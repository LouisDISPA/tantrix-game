//! Persistent room identity, invitation allocation, inactivity and retention
//! policy. Both adapters serialize access to one record; all time is supplied
//! by the host.

use crate::{
    room_logic::{CompletedMatch, RoomData},
    session_cookie::SessionToken,
};
use serde::{Deserialize, Serialize};
use std::future::Future;
use tantrix_proto::{
    RoomApiError, RoomReference, CREATE_RETRY_WINDOW_MS, ROOM_CODE_ALPHABET, ROOM_CODE_LEN,
};

const HOUR_MS: u64 = 60 * 60 * 1000;
const DAY_MS: u64 = 24 * HOUR_MS;
/// A lobby, or an invitation code, with nobody connected for this long expires.
pub const IDLE_TTL_MS: u64 = HOUR_MS;
/// An expired lobby's tombstone is deleted this long after the lobby expired.
pub const TOMBSTONE_TTL_MS: u64 = DAY_MS;
/// A room whose game started is deleted, with its match history, after
/// nobody has been connected for this long.
pub const RETENTION_MS: u64 = 3 * DAY_MS;
/// Codes [`allocate`] tries to claim before it answers `AllocationBusy`.
pub const ALLOCATION_ATTEMPTS: usize = 10;

// A lobby's inactivity starts no earlier than the request that created it,
// so clients stop retrying that request before the lobby can expire. The
// tombstone TTL is then a margin for client clocks set back.
const _: () = assert!(CREATE_RETRY_WINDOW_MS <= IDLE_TTL_MS);

/// Deterministic server allocation makes a lost response safe to retry, without
/// another directory or exposing the private session token.
pub fn game_id(token: &SessionToken, request_id: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("tantrix:{}:{request_id}", token.as_str()).as_bytes(),
    )
    .to_string()
}

/// A short invitation code, and the nonce that reserves it for one room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    /// What players type or share: [`ROOM_CODE_LEN`] letters of
    /// [`ROOM_CODE_ALPHABET`].
    pub code: String,
    /// Random nonce of this reservation, so a stale release can't free the
    /// code once it is claimed again.
    pub claim: String,
}

/// A random code with a fresh claim, not reserved yet. Fails with
/// `StorageUnavailable` if the platform RNG does.
pub fn new_invitation() -> Result<Invitation, RoomApiError> {
    let mut code = String::with_capacity(ROOM_CODE_LEN);
    while code.len() < ROOM_CODE_LEN {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| RoomApiError::StorageUnavailable)?;
        for byte in bytes {
            // Rejection sampling avoids favoring the first letters.
            if byte < 253 && code.len() < ROOM_CODE_LEN {
                code.push(ROOM_CODE_ALPHABET[byte as usize % ROOM_CODE_ALPHABET.len()] as char);
            }
        }
    }
    Ok(Invitation {
        code,
        claim: uuid::Uuid::new_v4().to_string(),
    })
}

/// What storage keeps for one room: its state, its invitation codes and
/// how long it has been vacant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRecord {
    /// The room's permanent game ID.
    pub id: String,
    /// None is an expired-lobby tombstone. Creation retries must not revive it.
    pub room: Option<RoomData>,
    /// The code reserved for this room, if any.
    pub invitation: Option<Invitation>,
    /// Persisted before reserving a code, so a crash can resume the same claim.
    pub pending: Option<Invitation>,
    /// Persisted before release, so a failed cleanup can be retried safely.
    pub retired: Vec<Invitation>,
    /// Since when nobody has been connected, in milliseconds since the Unix
    /// epoch; `None` while someone is. Expiry and retention count from it.
    pub vacant_since: Option<u64>,
}

impl RoomRecord {
    /// Archive only at a match transition, keeping history out of the active
    /// room record and its per-move clones/writes.
    pub fn replace_room(&mut self, room: &RoomData) -> Option<CompletedMatch> {
        let archive = self
            .room
            .as_ref()
            .filter(|old| old.match_id != room.match_id)
            .and_then(RoomData::completed_match);
        self.room = Some(room.clone());
        archive
    }

    /// A new empty lobby without a code, vacant since `now`.
    pub fn new(id: String, now: u64) -> Self {
        Self {
            id,
            room: Some(RoomData::default()),
            invitation: None,
            pending: None,
            retired: Vec::new(),
            vacant_since: Some(now),
        }
    }

    /// The room's ID and code for clients; `Expired` for a tombstone.
    pub fn reference(&self) -> Result<RoomReference, RoomApiError> {
        self.room.as_ref().ok_or(RoomApiError::Expired)?;
        Ok(RoomReference {
            id: self.id.clone(),
            code: self.invitation.as_ref().map(|i| i.code.clone()),
        })
    }

    /// Note whether anyone is connected; `true` if that changed the record.
    pub fn observe_connections(&mut self, connected: bool, now: u64) -> bool {
        let before = self.vacant_since;
        if connected {
            self.vacant_since = None;
        } else if self.vacant_since.is_none() {
            self.vacant_since = Some(now);
        }
        self.vacant_since != before
    }

    /// When the host must next run [`cleanup`], unless someone connects.
    pub fn deadline(&self) -> Option<u64> {
        self.expiry().into_iter().chain(self.deletion()).min()
    }

    /// When the invitation code is released and an unstarted lobby becomes
    /// a tombstone.
    fn expiry(&self) -> Option<u64> {
        let needs_expiry = self.invitation.is_some()
            || self.pending.is_some()
            || self.room.as_ref().is_some_and(|r| !r.has_started());
        needs_expiry
            .then_some(self.vacant_since)
            .flatten()
            .map(|t| t.saturating_add(IDLE_TTL_MS))
    }

    /// When storage deletes this record and its match archives: a tombstone
    /// [`TOMBSTONE_TTL_MS`] after its lobby expired, a started room after
    /// [`RETENTION_MS`] with nobody connected. An unstarted lobby first
    /// expires into a tombstone.
    fn deletion(&self) -> Option<u64> {
        let kept = match &self.room {
            None => IDLE_TTL_MS + TOMBSTONE_TTL_MS,
            Some(room) if room.has_started() => RETENTION_MS,
            Some(_) => return None,
        };
        self.vacant_since.map(|t| t.saturating_add(kept))
    }

    /// Whether the retention rule deletes this record at `now`.
    pub fn is_deletable(&self, now: u64) -> bool {
        self.deletion().is_some_and(|deletion| now >= deletion)
    }

    /// Once the record has been vacant for [`IDLE_TTL_MS`] at `now`, retire
    /// its codes and turn an unstarted lobby into a tombstone; `true` if it
    /// did.
    pub fn expire(&mut self, now: u64) -> bool {
        if !self.expiry().is_some_and(|expiry| now >= expiry) {
            return false;
        }
        self.retired.extend(self.invitation.take());
        self.retired.extend(self.pending.take());
        if self.room.as_ref().is_some_and(|r| !r.has_started()) {
            self.room = None;
        }
        true
    }
}

/// Each code has one strongly consistent owner. Release checks the claim nonce
/// as well as the game ID, so stale cleanup cannot remove a newer invitation.
pub trait Catalog {
    /// Durably store the record.
    fn save(&mut self, room: &RoomRecord) -> impl Future<Output = Result<(), RoomApiError>>;
    /// Reserve `invitation`'s code for room `id`: `true` if this room and
    /// claim now own the code, `false` if another claim does.
    fn claim(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> impl Future<Output = Result<bool, RoomApiError>>;
    /// Free `invitation`'s code if room `id` still holds it with this claim;
    /// otherwise leave it.
    fn release(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> impl Future<Output = Result<(), RoomApiError>>;
    /// Remove the room record and its match archives.
    fn delete(&mut self, id: &str) -> impl Future<Output = Result<(), RoomApiError>>;
}

/// Expire, release retired codes, then apply the retention rule. A deleted
/// record answers `NotFound`, like any missing room.
pub async fn cleanup(
    record: &mut RoomRecord,
    catalog: &mut impl Catalog,
    now: u64,
) -> Result<(), RoomApiError> {
    if record.expire(now) {
        catalog.save(record).await?;
    }
    while let Some(invitation) = record.retired.last() {
        catalog.release(&record.id, invitation).await?;
        record.retired.pop();
        catalog.save(record).await?;
    }
    if record.is_deletable(now) {
        catalog.delete(&record.id).await?;
        return Err(RoomApiError::NotFound);
    }
    Ok(())
}

/// Give the record an invitation code unless it has one. Each candidate is
/// saved as `pending` before it is claimed, so a stored pending claim is
/// resumed first. Fails with `Expired` for a tombstone, and with
/// `AllocationBusy` after [`ALLOCATION_ATTEMPTS`] taken codes.
pub async fn allocate(
    record: &mut RoomRecord,
    catalog: &mut impl Catalog,
    mut candidate: impl FnMut() -> Result<Invitation, RoomApiError>,
) -> Result<(), RoomApiError> {
    record.reference()?;
    if record.invitation.is_some() {
        return Ok(());
    }
    for _ in 0..ALLOCATION_ATTEMPTS {
        let invitation = match &record.pending {
            Some(pending) => pending.clone(),
            None => candidate()?,
        };
        record.pending = Some(invitation.clone());
        catalog.save(record).await?;
        if catalog.claim(&record.id, &invitation).await? {
            record.invitation = record.pending.take();
            catalog.save(record).await?;
            return Ok(());
        }
        record.pending = None;
        catalog.save(record).await?;
    }
    Err(RoomApiError::AllocationBusy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::collections::HashMap;

    #[test]
    fn a_lobby_between_matches_releases_its_code_but_keeps_the_room() {
        let mut record = RoomRecord::new("room".into(), 0);
        record.room.as_mut().unwrap().match_id = 1;
        record.invitation = Some(Invitation {
            code: "ABCDEF".into(),
            claim: "claim".into(),
        });
        assert!(record.expire(IDLE_TTL_MS));
        assert!(record.room.is_some());
        assert!(record.invitation.is_none());
        assert_eq!(record.deadline(), Some(RETENTION_MS));
        assert!(record.reference().is_ok());
    }

    #[derive(Default)]
    struct Memory {
        saved: Option<RoomRecord>,
        deleted: Vec<String>,
        claims: HashMap<String, (String, String)>,
        fail_save: bool,
        fail_after_claim: bool,
        fail_release: bool,
    }
    impl Catalog for Memory {
        async fn save(&mut self, room: &RoomRecord) -> Result<(), RoomApiError> {
            if self.fail_save {
                self.fail_save = false;
                return Err(RoomApiError::StorageUnavailable);
            }
            self.saved = Some(room.clone());
            Ok(())
        }
        async fn claim(&mut self, id: &str, i: &Invitation) -> Result<bool, RoomApiError> {
            let owner = self
                .claims
                .entry(i.code.clone())
                .or_insert_with(|| (id.into(), i.claim.clone()));
            let claimed = owner == &(id.into(), i.claim.clone());
            if self.fail_after_claim {
                self.fail_after_claim = false;
                self.fail_save = true;
            }
            Ok(claimed)
        }
        async fn release(&mut self, id: &str, i: &Invitation) -> Result<(), RoomApiError> {
            if self.fail_release {
                self.fail_release = false;
                return Err(RoomApiError::StorageUnavailable);
            }
            if self.claims.get(&i.code) == Some(&(id.into(), i.claim.clone())) {
                self.claims.remove(&i.code);
            }
            Ok(())
        }
        async fn delete(&mut self, id: &str) -> Result<(), RoomApiError> {
            self.deleted.push(id.into());
            Ok(())
        }
    }
    fn invitation(code: &str, claim: &str) -> Invitation {
        Invitation {
            code: code.into(),
            claim: claim.into(),
        }
    }

    #[test]
    fn creation_is_stable_and_scoped_to_the_session() {
        let a = SessionToken::new("a");
        let b = SessionToken::new("b");
        assert_eq!(game_id(&a, "request"), game_id(&a, "request"));
        assert_ne!(game_id(&a, "request"), game_id(&b, "request"));
        assert_ne!(game_id(&a, "request"), game_id(&a, "next"));
        assert!(tantrix_proto::is_game_id(&game_id(&a, "request")));
    }

    #[test]
    fn collision_tries_another_code_and_exhaustion_is_bounded() {
        block_on(async {
            let mut catalog = Memory::default();
            catalog
                .claims
                .insert("ABCDEF".into(), ("other".into(), "claim".into()));
            let mut record = RoomRecord::new("game".into(), 0);
            let mut candidates = [invitation("ABCDEF", "a"), invitation("BCDEFG", "b")].into_iter();
            allocate(&mut record, &mut catalog, || Ok(candidates.next().unwrap()))
                .await
                .unwrap();
            assert_eq!(record.reference().unwrap().code.as_deref(), Some("BCDEFG"));
            let mut second = RoomRecord::new("second".into(), 0);
            let mut tries = 0;
            assert_eq!(
                allocate(&mut second, &mut catalog, || {
                    tries += 1;
                    Ok(invitation("ABCDEF", "c"))
                })
                .await,
                Err(RoomApiError::AllocationBusy)
            );
            assert_eq!(tries, ALLOCATION_ATTEMPTS);
            assert_eq!(catalog.claims["ABCDEF"].0, "other");
        });
    }

    #[test]
    fn pending_claim_recovers_after_a_lost_commit() {
        block_on(async {
            let mut catalog = Memory::default();
            let mut record = RoomRecord::new("game".into(), 0);
            record.pending = Some(invitation("ABCDEF", "claim"));
            catalog
                .claims
                .insert("ABCDEF".into(), ("game".into(), "claim".into()));
            allocate(&mut record, &mut catalog, || {
                panic!("must resume the stored claim")
            })
            .await
            .unwrap();
            let reference = record.reference().unwrap();
            allocate(&mut record, &mut catalog, || panic!("must be idempotent"))
                .await
                .unwrap();
            assert_eq!(record.reference().unwrap(), reference);
        });
    }

    #[test]
    fn expiry_respects_presence_and_keeps_a_tombstone() {
        block_on(async {
            let mut catalog = Memory::default();
            let mut record = RoomRecord::new("game".into(), 100);
            allocate(&mut record, &mut catalog, || {
                Ok(invitation("ABCDEF", "claim"))
            })
            .await
            .unwrap();
            assert!(!record.expire(100 + IDLE_TTL_MS - 1));
            assert!(record.observe_connections(true, 200));
            assert!(!record.observe_connections(true, 250), "still connected");
            assert!(!record.expire(100 + IDLE_TTL_MS));
            assert!(record.observe_connections(false, 300));
            assert!(!record.observe_connections(false, 350), "vacant since 300");
            cleanup(&mut record, &mut catalog, 300 + IDLE_TTL_MS)
                .await
                .unwrap();
            assert_eq!(record.reference(), Err(RoomApiError::Expired));
            assert!(catalog.claims.is_empty());
            assert_eq!(
                allocate(&mut record, &mut catalog, || panic!(
                    "expired requests stay expired"
                ))
                .await,
                Err(RoomApiError::Expired)
            );
        });
    }

    #[test]
    fn stale_release_cannot_delete_a_reassigned_code() {
        block_on(async {
            let mut catalog = Memory::default();
            catalog
                .claims
                .insert("ABCDEF".into(), ("new-game".into(), "new-claim".into()));
            let mut record = RoomRecord::new("old-game".into(), 0);
            record.retired.push(invitation("ABCDEF", "old-claim"));
            cleanup(&mut record, &mut catalog, 0).await.unwrap();
            assert_eq!(catalog.claims["ABCDEF"].0, "new-game");
        });
    }

    #[test]
    fn failed_persistence_never_reserves_a_code() {
        block_on(async {
            let mut catalog = Memory {
                fail_save: true,
                ..Memory::default()
            };
            let mut record = RoomRecord::new("game".into(), 0);
            assert_eq!(
                allocate(&mut record, &mut catalog, || Ok(invitation(
                    "ABCDEF", "claim"
                )))
                .await,
                Err(RoomApiError::StorageUnavailable)
            );
            assert!(catalog.claims.is_empty());
        });
    }
    #[test]
    fn expiry_preserves_started_and_finished_games_and_their_history() {
        block_on(async {
            use tantrix_game::{Game, Player};
            for ended in [false, true] {
                let mut catalog = Memory::default();
                let mut record = RoomRecord::new("original-game".into(), 0);
                let game = Game::new_seeded(
                    Player::Someone,
                    Player::Someone,
                    Player::None,
                    Player::None,
                    42,
                )
                .unwrap();
                record.room.as_mut().unwrap().game = Some(game.to_data());
                record.room.as_mut().unwrap().ended = ended;
                let before = serde_json::to_value(record.room.as_ref().unwrap()).unwrap();
                allocate(&mut record, &mut catalog, || {
                    Ok(invitation("ABCDEF", "first"))
                })
                .await
                .unwrap();
                cleanup(&mut record, &mut catalog, IDLE_TTL_MS)
                    .await
                    .unwrap();
                assert_eq!(
                    serde_json::to_value(record.room.as_ref().unwrap()).unwrap(),
                    before
                );
                assert!(record.reference().unwrap().code.is_none());
                assert_eq!(record.deadline(), Some(RETENTION_MS));
                assert!(catalog.claims.is_empty());
                record.observe_connections(true, IDLE_TTL_MS + 1);
                allocate(&mut record, &mut catalog, || {
                    Ok(invitation("BCDEFG", "next"))
                })
                .await
                .unwrap();
                assert_eq!(record.reference().unwrap().id, "original-game");
                assert_eq!(record.reference().unwrap().code.as_deref(), Some("BCDEFG"));
                assert!(record.deadline().is_none());
            }
        });
    }

    #[test]
    fn failure_after_reservation_recovers_from_the_persisted_pending_record() {
        block_on(async {
            let mut catalog = Memory {
                fail_after_claim: true,
                ..Memory::default()
            };
            let mut record = RoomRecord::new("game".into(), 0);
            assert_eq!(
                allocate(&mut record, &mut catalog, || Ok(invitation(
                    "ABCDEF", "claim"
                )))
                .await,
                Err(RoomApiError::StorageUnavailable)
            );
            let mut recovered = catalog.saved.clone().unwrap();
            assert!(recovered.invitation.is_none());
            assert!(recovered.pending.is_some());
            allocate(&mut recovered, &mut catalog, || {
                panic!("reuse the pending claim")
            })
            .await
            .unwrap();
            assert_eq!(
                recovered.reference().unwrap().code.as_deref(),
                Some("ABCDEF")
            );
            assert_eq!(catalog.claims.len(), 1);
        });
    }
    #[test]
    fn an_interrupted_release_is_retried_even_after_the_lobby_is_expired() {
        block_on(async {
            let mut catalog = Memory::default();
            let mut record = RoomRecord::new("game".into(), 0);
            allocate(&mut record, &mut catalog, || {
                Ok(invitation("ABCDEF", "claim"))
            })
            .await
            .unwrap();
            catalog.fail_release = true;
            assert_eq!(
                cleanup(&mut record, &mut catalog, IDLE_TTL_MS).await,
                Err(RoomApiError::StorageUnavailable)
            );
            let mut recovered = catalog.saved.clone().unwrap();
            assert_eq!(recovered.reference(), Err(RoomApiError::Expired));
            assert_eq!(recovered.retired.len(), 1);
            cleanup(&mut recovered, &mut catalog, IDLE_TTL_MS)
                .await
                .unwrap();
            assert!(catalog.claims.is_empty());
            assert!(recovered.retired.is_empty());
        });
    }

    #[test]
    fn a_tombstone_is_deleted_once_no_creation_retry_can_reach_it() {
        block_on(async {
            let mut catalog = Memory::default();
            let mut record = RoomRecord::new("game".into(), 100);
            let expired = 100 + IDLE_TTL_MS;
            cleanup(&mut record, &mut catalog, expired).await.unwrap();
            assert_eq!(record.reference(), Err(RoomApiError::Expired));
            let deletion = expired + TOMBSTONE_TTL_MS;
            assert_eq!(record.deadline(), Some(deletion));
            cleanup(&mut record, &mut catalog, deletion - 1)
                .await
                .unwrap();
            assert!(catalog.deleted.is_empty());
            assert_eq!(
                cleanup(&mut record, &mut catalog, deletion).await,
                Err(RoomApiError::NotFound)
            );
            assert_eq!(catalog.deleted, ["game"]);
        });
    }

    #[test]
    fn a_started_room_is_deleted_after_long_inactivity_and_presence_resets_the_clock() {
        block_on(async {
            let mut catalog = Memory::default();
            let mut record = RoomRecord::new("game".into(), 0);
            record.room.as_mut().unwrap().match_id = 1;
            assert_eq!(record.deadline(), Some(RETENTION_MS));
            assert!(record.observe_connections(true, RETENTION_MS - 1));
            assert_eq!(record.deadline(), None);
            assert!(!record.is_deletable(u64::MAX), "never while connected");
            let left = RETENTION_MS + 5;
            assert!(record.observe_connections(false, left));
            cleanup(&mut record, &mut catalog, left + RETENTION_MS - 1)
                .await
                .unwrap();
            assert_eq!(
                cleanup(&mut record, &mut catalog, left + RETENTION_MS).await,
                Err(RoomApiError::NotFound)
            );
            assert_eq!(catalog.deleted, ["game"]);
        });
    }

    #[test]
    fn a_late_sweep_releases_codes_before_deleting() {
        block_on(async {
            for match_id in [0, 1] {
                let mut catalog = Memory::default();
                let mut record = RoomRecord::new("game".into(), 0);
                record.room.as_mut().unwrap().match_id = match_id;
                allocate(&mut record, &mut catalog, || {
                    Ok(invitation("ABCDEF", "claim"))
                })
                .await
                .unwrap();
                let late = IDLE_TTL_MS + TOMBSTONE_TTL_MS + RETENTION_MS;
                catalog.fail_release = true;
                assert_eq!(
                    cleanup(&mut record, &mut catalog, late).await,
                    Err(RoomApiError::StorageUnavailable)
                );
                assert!(
                    catalog.deleted.is_empty(),
                    "a claimed code is never orphaned"
                );
                assert_eq!(
                    cleanup(&mut record, &mut catalog, late).await,
                    Err(RoomApiError::NotFound)
                );
                assert!(catalog.claims.is_empty());
                assert_eq!(catalog.deleted, ["game"]);
            }
        });
    }

    #[test]
    fn an_unstarted_lobby_is_only_deleted_as_a_tombstone() {
        let mut record = RoomRecord::new("game".into(), 0);
        assert!(!record.is_deletable(u64::MAX));
        assert_eq!(record.deadline(), Some(IDLE_TTL_MS));
        record.room.as_mut().unwrap().match_id = 1;
        assert!(record.is_deletable(RETENTION_MS), "between matches");
    }
}
