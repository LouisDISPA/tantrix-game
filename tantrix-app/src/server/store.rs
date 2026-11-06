//! SQLite room records and atomic invitation claims. Game IDs never change.

use rusqlite::{Connection, OptionalExtension};
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tantrix_proto::RoomApiError;
use tantrix_room::lifecycle::{Catalog, Invitation, RoomRecord};
use tantrix_room::room_logic::CompletedMatch;

pub type StoreError = Box<dyn Error + Send + Sync>;
pub type StoreResult<T> = Result<T, StoreError>;

/// A record as its `online_games` row stores it.
struct Row {
    id: String,
    json: String,
    /// When the sweep next cleans the record; codes still to release make it due now.
    deadline: Option<u64>,
}

impl Row {
    fn new(record: &RoomRecord) -> StoreResult<Self> {
        Ok(Self {
            id: record.id.clone(),
            json: serde_json::to_string(record)?,
            deadline: if record.retired.is_empty() {
                record.deadline()
            } else {
                Some(0)
            },
        })
    }

    /// Overwrite the stored row; false if it is gone.
    fn update(&self, conn: &Connection) -> rusqlite::Result<bool> {
        Ok(conn.execute(
            "UPDATE online_games SET record_json = ?2, deadline = ?3 WHERE id = ?1",
            (&self.id, &self.json, self.deadline),
        )? == 1)
    }
}

#[derive(Clone)]
pub struct RoomStore {
    connection: Arc<Mutex<Connection>>,
}

impl RoomStore {
    pub fn open(path: impl Into<PathBuf>) -> StoreResult<Self> {
        let path = path.into();
        let conn = if path == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(&path)?
        };
        conn.busy_timeout(Duration::from_secs(5))?;
        if path != Path::new(":memory:") {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        conn.execute_batch("CREATE TABLE IF NOT EXISTS online_games (id TEXT PRIMARY KEY, record_json TEXT NOT NULL, deadline INTEGER);
            CREATE INDEX IF NOT EXISTS online_games_deadline ON online_games(deadline);
            CREATE TABLE IF NOT EXISTS online_codes (code TEXT PRIMARY KEY, game_id TEXT NOT NULL, claim TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS online_matches (room_id TEXT NOT NULL, match_id INTEGER NOT NULL, state_json TEXT NOT NULL, PRIMARY KEY (room_id, match_id));")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(conn)),
        })
    }

    async fn with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> StoreResult<T> + Send + 'static,
    ) -> StoreResult<T> {
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let conn = connection
                .lock()
                .map_err(|_| std::io::Error::other("room database lock poisoned"))?;
            f(&conn)
        })
        .await?
    }

    pub async fn insert(&self, id: &str, now: u64) -> StoreResult<()> {
        let row = Row::new(&RoomRecord::new(id.into(), now))?;
        self.with(move |conn| {
            conn.execute("INSERT OR IGNORE INTO online_games (id, record_json, deadline) VALUES (?1, ?2, ?3)", (&row.id, &row.json, row.deadline))?;
            Ok(())
        }).await
    }

    pub async fn load(&self, id: &str) -> StoreResult<Option<RoomRecord>> {
        let id = id.to_owned();
        self.with(move |conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT record_json FROM online_games WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .optional()?;
            json.map(|json| serde_json::from_str(&json).map_err(Into::into))
                .transpose()
        })
        .await
    }

    pub async fn save_record(&self, record: &RoomRecord) -> StoreResult<()> {
        self.save_with_archive(record, None).await
    }

    pub async fn save_with_archive(
        &self,
        record: &RoomRecord,
        archive: Option<CompletedMatch>,
    ) -> StoreResult<()> {
        let row = Row::new(record)?;
        self.with(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            if let Some(archive) = archive {
                transaction.execute("INSERT OR IGNORE INTO online_matches (room_id, match_id, state_json) VALUES (?1, ?2, ?3)", (&row.id, archive.id, serde_json::to_string(&archive)?))?;
            }
            if !row.update(&transaction)? {
                return Err(std::io::Error::other("room disappeared from storage").into());
            }
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    #[cfg(test)]
    pub async fn load_match(
        &self,
        room_id: &str,
        match_id: u32,
    ) -> StoreResult<Option<CompletedMatch>> {
        let room_id = room_id.to_owned();
        self.with(move |conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT state_json FROM online_matches WHERE room_id = ?1 AND match_id = ?2",
                    (&room_id, match_id),
                    |row| row.get(0),
                )
                .optional()?;
            json.map(|json| serde_json::from_str(&json).map_err(Into::into))
                .transpose()
        })
        .await
    }

    pub async fn find_code(&self, code: &str) -> StoreResult<Option<String>> {
        let code = code.to_owned();
        self.with(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT game_id FROM online_codes WHERE code = ?1",
                    [&code],
                    |row| row.get(0),
                )
                .optional()?)
        })
        .await
    }

    /// No socket survives a restart: rooms saved as connected (the rows
    /// without a deadline) become vacant from `now`, which schedules cleanup.
    pub async fn mark_vacant(&self, now: u64) -> StoreResult<()> {
        self.with(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            let records: Vec<String> = transaction
                .prepare("SELECT record_json FROM online_games WHERE deadline IS NULL")?
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            for json in records {
                let mut record: RoomRecord = serde_json::from_str(&json)?;
                record.observe_connections(false, now);
                Row::new(&record)?.update(&transaction)?;
            }
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn due(&self, now: u64) -> StoreResult<Vec<String>> {
        self.with(move |conn| {
            let mut query = conn.prepare("SELECT id FROM online_games WHERE deadline <= ?1")?;
            let ids = query
                .query_map([now], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(ids)
        })
        .await
    }
}

pub fn unavailable<T>(result: StoreResult<T>) -> Result<T, RoomApiError> {
    result.map_err(|error| {
        log::error!("room storage error: {error}");
        RoomApiError::StorageUnavailable
    })
}

impl Catalog for RoomStore {
    async fn save(&mut self, record: &RoomRecord) -> Result<(), RoomApiError> {
        unavailable(self.save_record(record).await)
    }
    async fn claim(&mut self, id: &str, invitation: &Invitation) -> Result<bool, RoomApiError> {
        let id = id.to_owned();
        let invitation = invitation.clone();
        unavailable(
            self.with(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO online_codes (code, game_id, claim) VALUES (?1, ?2, ?3)",
                    (&invitation.code, &id, &invitation.claim),
                )?;
                let owner: (String, String) = conn.query_row(
                    "SELECT game_id, claim FROM online_codes WHERE code = ?1",
                    [&invitation.code],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                Ok(owner == (id, invitation.claim))
            })
            .await,
        )
    }
    async fn release(&mut self, id: &str, invitation: &Invitation) -> Result<(), RoomApiError> {
        let id = id.to_owned();
        let invitation = invitation.clone();
        unavailable(
            self.with(move |conn| {
                conn.execute(
                    "DELETE FROM online_codes WHERE code = ?1 AND game_id = ?2 AND claim = ?3",
                    (&invitation.code, &id, &invitation.claim),
                )?;
                Ok(())
            })
            .await,
        )
    }
    async fn delete(&mut self, id: &str) -> Result<(), RoomApiError> {
        let id = id.to_owned();
        unavailable(
            self.with(move |conn| {
                let transaction = conn.unchecked_transaction()?;
                transaction.execute("DELETE FROM online_matches WHERE room_id = ?1", [&id])?;
                transaction.execute("DELETE FROM online_games WHERE id = ?1", [&id])?;
                transaction.commit()?;
                Ok(())
            })
            .await,
        )
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use tantrix_game::{Color, Game, Player};
    use tantrix_proto::EndReason;
    use tantrix_room::lifecycle::{self, IDLE_TTL_MS, RETENTION_MS};

    /// The archive of match 0.
    fn archive() -> CompletedMatch {
        let game = Game::new_seeded(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            42,
        )
        .unwrap();
        CompletedMatch {
            id: 0,
            game: game.to_data(),
            seats: Default::default(),
            reason: EndReason::Completed {
                winners: vec![Color::Red],
            },
        }
    }

    /// A stored room between matches, with match 0 archived.
    pub async fn finished_room(store: &RoomStore, id: &str, now: u64) -> RoomRecord {
        store.insert(id, now).await.unwrap();
        let mut record = store.load(id).await.unwrap().unwrap();
        record.room.as_mut().unwrap().match_id = 1;
        store
            .save_with_archive(&record, Some(archive()))
            .await
            .unwrap();
        record
    }

    #[tokio::test]
    async fn a_failed_room_update_rolls_back_the_archive_too() {
        let store = RoomStore::open(":memory:").unwrap();
        let archive = archive();
        let missing = RoomRecord::new("missing".into(), 0);
        assert!(store
            .save_with_archive(&missing, Some(archive))
            .await
            .is_err());
        assert!(store.load_match("missing", 0).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn allocation_survives_reopen_and_expiry_releases_only_the_claim() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.sqlite3");
        let mut store = RoomStore::open(&path).unwrap();
        store.insert("game", 0).await.unwrap();
        let mut record = store.load("game").await.unwrap().unwrap();
        lifecycle::allocate(&mut record, &mut store, || {
            Ok(Invitation {
                code: "ABCDEF".into(),
                claim: "one".into(),
            })
        })
        .await
        .unwrap();
        drop(store);
        let mut store = RoomStore::open(&path).unwrap();
        assert_eq!(
            store.find_code("ABCDEF").await.unwrap().as_deref(),
            Some("game")
        );
        let mut record = store.load("game").await.unwrap().unwrap();
        lifecycle::cleanup(&mut record, &mut store, IDLE_TTL_MS)
            .await
            .unwrap();
        assert!(store.find_code("ABCDEF").await.unwrap().is_none());
        assert!(store.due(IDLE_TTL_MS).await.unwrap().is_empty());
        store.insert("game", IDLE_TTL_MS).await.unwrap();
        assert_eq!(
            store.load("game").await.unwrap().unwrap().reference(),
            Err(RoomApiError::Expired)
        );
        store.insert("next", IDLE_TTL_MS).await.unwrap();
        let mut next = store.load("next").await.unwrap().unwrap();
        lifecycle::allocate(&mut next, &mut store, || {
            Ok(Invitation {
                code: "ABCDEF".into(),
                claim: "two".into(),
            })
        })
        .await
        .unwrap();
        store
            .release(
                "game",
                &Invitation {
                    code: "ABCDEF".into(),
                    claim: "one".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.find_code("ABCDEF").await.unwrap().as_deref(),
            Some("next")
        );
    }

    #[tokio::test]
    async fn retention_deletes_the_room_row_and_its_match_archives() {
        let mut store = RoomStore::open(":memory:").unwrap();
        let mut record = finished_room(&store, "game", 0).await;
        finished_room(&store, "other", 1).await;
        assert_eq!(
            store.due(RETENTION_MS - 1).await.unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(store.due(RETENTION_MS).await.unwrap(), ["game"]);
        assert_eq!(
            lifecycle::cleanup(&mut record, &mut store, RETENTION_MS).await,
            Err(RoomApiError::NotFound)
        );
        assert!(store.load("game").await.unwrap().is_none());
        assert!(store.load_match("game", 0).await.unwrap().is_none());
        assert_eq!(store.due(10 * RETENTION_MS).await.unwrap(), ["other"]);
        assert!(store.load_match("other", 0).await.unwrap().is_some());
        assert!(
            store.save_record(&record).await.is_err(),
            "never resurrected"
        );
    }

    #[tokio::test]
    async fn rooms_left_connected_get_a_deadline_at_startup() {
        let store = RoomStore::open(":memory:").unwrap();
        let mut record = finished_room(&store, "game", 0).await;
        record.observe_connections(true, 5);
        store.save_record(&record).await.unwrap();
        assert!(store.due(10 * RETENTION_MS).await.unwrap().is_empty());
        store.mark_vacant(1_000).await.unwrap();
        assert!(store
            .due(1_000 + RETENTION_MS - 1)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(store.due(1_000 + RETENTION_MS).await.unwrap(), ["game"]);
    }
}
