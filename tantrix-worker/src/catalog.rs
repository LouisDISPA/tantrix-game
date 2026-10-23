//! One strongly consistent object per short invitation code.
use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tantrix_proto::RoomApiError;
use tantrix_room::lifecycle::{Catalog, Invitation, RoomRecord};
use worker::{
    durable_object, DurableObject, Env, Method, Request, Response, Result, State, Storage,
};

pub fn now_ms() -> u64 {
    worker::Date::now().as_millis()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub id: String,
    pub claim: String,
}

#[durable_object]
pub struct RoomCodes {
    state: State,
    lock: Mutex<()>,
}
impl DurableObject for RoomCodes {
    fn new(state: State, _env: Env) -> Self {
        Self {
            state,
            lock: Mutex::new(()),
        }
    }
    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let _guard = self.lock.lock().await;
        let storage = self.state.storage();
        let owner = storage.get::<Owner>("owner").await?;
        if req.method() == Method::Get {
            return Response::from_json(&owner);
        }
        let proposed: Owner = req.json().await?;
        match req.method() {
            Method::Put => {
                if let Some(owner) = owner {
                    Response::from_json(&(owner == proposed))
                } else {
                    storage.put("owner", proposed).await?;
                    Response::from_json(&true)
                }
            }
            Method::Delete => {
                if owner.as_ref() == Some(&proposed) {
                    storage.delete("owner").await?;
                }
                Response::from_json(&true)
            }
            _ => Response::error("method not allowed", 405),
        }
    }
}

pub struct WorkerCatalog {
    pub storage: Storage,
    pub env: Env,
}

pub fn unavailable(error: worker::Error) -> RoomApiError {
    worker::console_error!("room storage error: {error}");
    RoomApiError::StorageUnavailable
}

pub async fn persist(storage: &Storage, record: &RoomRecord) -> Result<()> {
    // Schedule before writing: a failed write must not strand an idle record
    // without an alarm. An obsolete alarm simply rechecks current presence.
    let deadline = if record.retired.is_empty() {
        record.deadline()
    } else {
        Some(now_ms() + 60_000)
    };
    if let Some(deadline) = deadline {
        storage
            .set_alarm(std::time::Duration::from_millis(
                deadline.saturating_sub(now_ms()).max(1),
            ))
            .await?;
    }
    storage.put("record", record).await
}

impl WorkerCatalog {
    async fn send(&self, id: &str, invitation: &Invitation, method: Method) -> Result<bool> {
        let body = serde_json::to_string(&Owner {
            id: id.into(),
            claim: invitation.claim.clone(),
        })?;
        let mut init = worker::RequestInit::new();
        init.with_method(method).with_body(Some(body.into()));
        let request = Request::new_with_init("https://code/", &init)?;
        let mut response = self
            .env
            .durable_object("ROOM_CODES")?
            .get_by_name(&invitation.code)?
            .fetch_with_request(request)
            .await?;
        if response.status_code() != 200 {
            return Err(worker::Error::RustError("code storage unavailable".into()));
        }
        response.json().await
    }
}
impl Catalog for WorkerCatalog {
    async fn save(&mut self, record: &RoomRecord) -> std::result::Result<(), RoomApiError> {
        persist(&self.storage, record).await.map_err(unavailable)
    }
    async fn claim(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> std::result::Result<bool, RoomApiError> {
        self.send(id, invitation, Method::Put)
            .await
            .map_err(unavailable)
    }
    async fn release(
        &mut self,
        id: &str,
        invitation: &Invitation,
    ) -> std::result::Result<(), RoomApiError> {
        self.send(id, invitation, Method::Delete)
            .await
            .map(|_| ())
            .map_err(unavailable)
    }
}
