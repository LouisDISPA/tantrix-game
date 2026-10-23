//! Persistent room identity, invitation allocation and inactivity policy.
//! Both adapters serialize access to one record; all time is supplied by the host.

use crate::{room_logic::RoomData, session_cookie::SessionToken};
use serde::{Deserialize, Serialize};
use std::future::Future;
use tantrix_proto::{RoomApiError, RoomReference, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

pub const IDLE_TTL_MS: u64 = 24 * 60 * 60 * 1000;
pub const ALLOCATION_ATTEMPTS: usize = 10;

/// Deterministic server allocation makes a lost response safe to retry, without
/// another directory or exposing the private session token.
pub fn game_id(token: &SessionToken, request_id: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("tantrix:{}:{request_id}", token.as_str()).as_bytes(),
    )
    .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub code: String,
    pub claim: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRecord {
    pub id: String,
    /// None is an expired-lobby tombstone. Creation retries must not revive it.
    pub room: Option<RoomData>,
    pub invitation: Option<Invitation>,
    /// Persisted before reserving a code, so a crash can resume the same claim.
    pub pending: Option<Invitation>,
    /// Persisted before release, so a failed cleanup can be retried safely.
    pub retired: Vec<Invitation>,
    pub vacant_since: Option<u64>,
}

impl RoomRecord {
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

    pub fn reference(&self) -> Result<RoomReference, RoomApiError> {
        self.room.as_ref().ok_or(RoomApiError::Expired)?;
        Ok(RoomReference {
            id: self.id.clone(),
            code: self.invitation.as_ref().map(|i| i.code.clone()),
        })
    }

    pub fn observe_connections(&mut self, connected: bool, now: u64) {
        if connected {
            self.vacant_since = None;
        } else if self.vacant_since.is_none() {
            self.vacant_since = Some(now);
        }
    }

    pub fn deadline(&self) -> Option<u64> {
        let needs_expiry = self.invitation.is_some()
            || self.pending.is_some()
            || self.room.as_ref().is_some_and(|r| r.game.is_none());
        needs_expiry
            .then_some(self.vacant_since)
            .flatten()
            .map(|t| t.saturating_add(IDLE_TTL_MS))
    }

    pub fn expire(&mut self, now: u64) -> bool {
        if !self.deadline().is_some_and(|deadline| now >= deadline) {
            return false;
        }
        self.retired.extend(self.invitation.take());
        self.retired.extend(self.pending.take());
        if self.room.as_ref().is_some_and(|r| r.game.is_none()) {
            self.room = None;
        }
        true
    }
}

/// Each code has one strongly consistent owner. Release checks the claim nonce
/// as well as the game ID, so stale cleanup cannot remove a newer invitation.
pub trait Catalog {
    fn save(&mut self, room: &RoomRecord) -> impl Future<Output = Result<(), RoomApiError>>;
    fn claim(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> impl Future<Output = Result<bool, RoomApiError>>;
    fn release(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> impl Future<Output = Result<(), RoomApiError>>;
}

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
    Ok(())
}

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

    #[derive(Default)]
    struct Memory {
        saved: Option<RoomRecord>,
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
            record.observe_connections(true, 200);
            assert!(!record.expire(100 + IDLE_TTL_MS));
            record.observe_connections(false, 300);
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
                assert!(record.deadline().is_none());
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
}
