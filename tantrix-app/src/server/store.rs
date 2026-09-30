//! SQLite persistence for native rooms. Live sockets never enter storage.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use tantrix_room::room_logic::RoomData;

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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rooms (code TEXT PRIMARY KEY, state_json TEXT NOT NULL)",
            [],
        )?;
        Ok(Self {
            connection: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock(connection: &Mutex<Connection>) -> StoreResult<std::sync::MutexGuard<'_, Connection>> {
        connection
            .lock()
            .map_err(|_| std::io::Error::other("room database lock poisoned").into())
    }

    pub async fn create(&self, code: &str) -> StoreResult<bool> {
        let connection = self.connection.clone();
        let code = code.to_owned();
        tokio::task::spawn_blocking(move || -> StoreResult<bool> {
            let conn = Self::lock(&connection)?;
            let state = serde_json::to_string(&RoomData::default())?;
            Ok(conn.execute(
                "INSERT OR IGNORE INTO rooms (code, state_json) VALUES (?1, ?2)",
                (&code, &state),
            )? == 1)
        })
        .await?
    }

    pub async fn load(&self, code: &str) -> StoreResult<Option<RoomData>> {
        let connection = self.connection.clone();
        let code = code.to_owned();
        tokio::task::spawn_blocking(move || -> StoreResult<Option<RoomData>> {
            let conn = Self::lock(&connection)?;
            let json: Option<String> = conn
                .query_row(
                    "SELECT state_json FROM rooms WHERE code = ?1",
                    [&code],
                    |row| row.get(0),
                )
                .optional()?;
            json.map(|json| serde_json::from_str(&json).map_err(Into::into))
                .transpose()
        })
        .await?
    }

    pub async fn save(&self, code: &str, room: &RoomData) -> StoreResult<()> {
        let connection = self.connection.clone();
        let code = code.to_owned();
        let json = serde_json::to_string(room)?;
        tokio::task::spawn_blocking(move || -> StoreResult<()> {
            let conn = Self::lock(&connection)?;
            let updated = conn.execute(
                "UPDATE rooms SET state_json = ?2 WHERE code = ?1",
                (&code, &json),
            )?;
            if updated != 1 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "room disappeared from storage",
                )
                .into());
            }
            Ok(())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::RoomStore;

    #[tokio::test]
    async fn in_memory_room_survives_across_storage_operations() {
        let store = RoomStore::open(":memory:").unwrap();
        assert!(store.create("ABCDEF").await.unwrap());
        assert!(!store.create("ABCDEF").await.unwrap());
        let mut room = store.load("ABCDEF").await.unwrap().unwrap();
        room.ended = true;
        store.save("ABCDEF", &room).await.unwrap();
        assert!(store.clone().load("ABCDEF").await.unwrap().unwrap().ended);
    }
}
