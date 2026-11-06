//! HTTP allocation and one pending creation request per server. Only permanent
//! IDs reach the WebSocket; reconnects never resolve invitation codes again.

use tantrix_proto::{CreateRoom, RoomApiError, RoomReference, CREATE_RETRY_WINDOW_MS};

fn new_request_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Could not create a room request. Please try again.".to_string())?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut id = String::with_capacity(36);
    for (i, byte) in bytes.iter().enumerate() {
        if [4, 6, 8, 10].contains(&i) {
            id.push('-');
        }
        id.push_str(&format!("{byte:02x}"));
    }
    Ok(id)
}

fn pending_key(server: &str) -> String {
    format!(
        "pending-room-{}",
        server
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

/// The request ID in a stored `"<id> <made at ms>"`, while the server still
/// remembers what it created. A clock set back before `made at` ends it, and
/// so does an older bare ID.
fn retryable(stored: &str, now: u64) -> Option<&str> {
    let (id, made_at) = stored.split_once(' ')?;
    let age = now.checked_sub(made_at.parse().ok()?)?;
    (tantrix_proto::is_game_id(id) && age < CREATE_RETRY_WINDOW_MS).then_some(id)
}

fn pending_request(server: &str) -> Result<String, String> {
    let key = pending_key(server);
    let now = web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if let Some(id) = load(&key)
        .as_deref()
        .and_then(|stored| retryable(stored, now))
    {
        return Ok(id.to_owned());
    }
    let id = new_request_id()?;
    save(&key, &format!("{id} {now}"))?;
    Ok(id)
}

pub async fn create(server: &str) -> Result<RoomReference, String> {
    let request = CreateRoom {
        request_id: pending_request(server)?,
    };
    let body = serde_json::to_string(&request).expect("room request serializes");
    let response = request_http(server, "/api/rooms", Some(&body)).await?;
    match decode_response(response) {
        Ok(room) => {
            save(&pending_key(server), "")?;
            Ok(room)
        }
        Err(error) => {
            if matches!(error, RoomApiError::Expired | RoomApiError::InvalidRequest) {
                save(&pending_key(server), "")?;
            }
            Err(error.message().into())
        }
    }
}

pub async fn resolve(server: &str, code: &str) -> Result<RoomReference, String> {
    decode_response(request_http(server, &format!("/api/rooms/code/{code}"), None).await?)
        .map_err(|error| error.message().into())
}

fn decode_response((status, body): (u16, String)) -> Result<RoomReference, RoomApiError> {
    if (200..300).contains(&status) {
        serde_json::from_str(&body).map_err(|_| RoomApiError::StorageUnavailable)
    } else {
        // A proxy in front of the server may answer 429 with its own body.
        let fallback = if status == 429 {
            RoomApiError::RateLimited
        } else {
            RoomApiError::StorageUnavailable
        };
        Err(serde_json::from_str(&body).unwrap_or(fallback))
    }
}

#[cfg(web_client)]
async fn request_http(
    server: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, String), String> {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{AbortSignal, Request, RequestInit, Response};
    let failed = || "Couldn't reach the game server. Please try again.".to_string();
    let init = RequestInit::new();
    init.set_method(if body.is_some() { "POST" } else { "GET" });
    init.set_signal(Some(&AbortSignal::timeout_with_u32(20_000)));
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    let request =
        Request::new_with_str_and_init(&format!("{}{path}", server.trim_end_matches('/')), &init)
            .map_err(|_| failed())?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|_| failed())?;
    let window = web_sys::window().ok_or_else(failed)?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| failed())?
        .dyn_into()
        .map_err(|_| failed())?;
    let status = response.status();
    let text = JsFuture::from(response.text().map_err(|_| failed())?)
        .await
        .map_err(|_| failed())?
        .as_string()
        .ok_or_else(failed)?;
    Ok((status, text))
}

#[cfg(native_io)]
async fn request_http(
    server: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, String), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}{path}", server.trim_end_matches('/'));
    let mut request = if let Some(body) = body {
        client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_owned())
    } else {
        client.get(url)
    };
    request = request.header("Cookie", super::server_settings::session_cookie(server)?);
    let response = request
        .send()
        .await
        .map_err(|_| "Couldn't reach the game server. Please try again.".to_string())?;
    let status = response.status().as_u16();
    Ok((status, response.text().await.map_err(|e| e.to_string())?))
}

#[cfg(not(any(web_client, native_io)))]
async fn request_http(_: &str, _: &str, _: Option<&str>) -> Result<(u16, String), String> {
    Err("Online rooms require a client build.".into())
}

#[cfg(web_client)]
fn load(key: &str) -> Option<String> {
    web_sys::window()?
        .session_storage()
        .ok()??
        .get_item(key)
        .ok()
        .flatten()
}
#[cfg(web_client)]
fn save(key: &str, value: &str) -> Result<(), String> {
    web_sys::window()
        .and_then(|w| w.session_storage().ok().flatten())
        .ok_or_else(|| "Enable browser storage to create a room.".to_string())?
        .set_item(key, value)
        .map_err(|_| "Could not save the pending room request.".into())
}
#[cfg(native_io)]
fn load(key: &str) -> Option<String> {
    super::native_settings::load(key)
}
#[cfg(native_io)]
fn save(key: &str, value: &str) -> Result<(), String> {
    super::native_settings::save(key, value)
}
#[cfg(not(any(web_client, native_io)))]
fn load(_: &str) -> Option<String> {
    None
}
#[cfg(not(any(web_client, native_io)))]
fn save(_: &str, _: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_ids_and_server_keys_are_unambiguous() {
        assert!(tantrix_proto::is_game_id(&new_request_id().unwrap()));
        assert_ne!(
            pending_key("https://one.example"),
            pending_key("https://two.example")
        );
        assert_eq!(
            decode_response((410, "\"Expired\"".into())),
            Err(RoomApiError::Expired)
        );
    }

    #[test]
    fn a_pending_request_is_retried_only_within_the_window() {
        let id = "01234567-89ab-4def-8123-456789abcdef";
        let stored = format!("{id} 1000");
        assert_eq!(retryable(&stored, 1000), Some(id));
        assert_eq!(
            retryable(&stored, 1000 + CREATE_RETRY_WINDOW_MS - 1),
            Some(id)
        );
        assert_eq!(retryable(&stored, 1000 + CREATE_RETRY_WINDOW_MS), None);
        assert_eq!(retryable(&stored, 999), None, "clock set back");
        assert_eq!(retryable(id, 1000), None, "stored without a time");
        assert_eq!(retryable("", 1000), None);
        assert_eq!(retryable("not-an-id 1000", 1000), None);
    }

    #[test]
    fn rate_limited_requests_show_a_short_retry_message() {
        for body in ["\"RateLimited\"", "Too Many Requests"] {
            assert_eq!(
                decode_response((429, body.into())),
                Err(RoomApiError::RateLimited)
            );
        }
        assert_eq!(
            RoomApiError::RateLimited.message(),
            "Too many room requests. Wait a minute and try again."
        );
    }
}
