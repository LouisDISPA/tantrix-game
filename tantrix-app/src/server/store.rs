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

pub type StoreError = Box<dyn Error + Send + Sync>;
pub type StoreResult<T> = Result<T, StoreError>;

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
            CREATE TABLE IF NOT EXISTS online_codes (code TEXT PRIMARY KEY, game_id TEXT NOT NULL, claim TEXT NOT NULL);")?;
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
        let record = RoomRecord::new(id.into(), now);
        let json = serde_json::to_string(&record)?;
        self.with(move |conn| {
            conn.execute("INSERT OR IGNORE INTO online_games (id, record_json, deadline) VALUES (?1, ?2, ?3)", (&record.id, &json, record.deadline()))?;
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
        let json = serde_json::to_string(record)?;
        let id = record.id.clone();
        let deadline = if record.retired.is_empty() {
            record.deadline()
        } else {
            Some(0)
        };
        self.with(move |conn| {
            if conn.execute(
                "UPDATE online_games SET record_json = ?2, deadline = ?3 WHERE id = ?1",
                (&id, &json, deadline),
            )? != 1
            {
                return Err(std::io::Error::other("room disappeared from storage").into());
            }
            Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantrix_room::lifecycle::{self, IDLE_TTL_MS};

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
}
