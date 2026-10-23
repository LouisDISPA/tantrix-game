//! HTTP allocation and one pending creation request per server. Only permanent
//! IDs reach the WebSocket; reconnects never resolve invitation codes again.

use tantrix_proto::{CreateRoom, RoomApiError, RoomReference};

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

fn pending_request(server: &str) -> Result<String, String> {
    let key = pending_key(server);
    if let Some(id) = load(&key).filter(|id| tantrix_proto::is_game_id(id)) {
        return Ok(id);
    }
    let id = new_request_id()?;
    save(&key, &id)?;
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
        Err(serde_json::from_str(&body).unwrap_or(RoomApiError::StorageUnavailable))
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
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

#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
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

#[cfg(not(any(
    all(feature = "web", target_arch = "wasm32"),
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
)))]
async fn request_http(_: &str, _: &str, _: Option<&str>) -> Result<(u16, String), String> {
    Err("Online rooms require a client build.".into())
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn load(key: &str) -> Option<String> {
    web_sys::window()?
        .session_storage()
        .ok()??
        .get_item(key)
        .ok()
        .flatten()
}
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn save(key: &str, value: &str) -> Result<(), String> {
    web_sys::window()
        .and_then(|w| w.session_storage().ok().flatten())
        .ok_or_else(|| "Enable browser storage to create a room.".to_string())?
        .set_item(key, value)
        .map_err(|_| "Could not save the pending room request.".into())
}
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
fn load(key: &str) -> Option<String> {
    super::native_settings::load(key)
}
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
fn save(key: &str, value: &str) -> Result<(), String> {
    super::native_settings::save(key, value)
}
#[cfg(not(any(
    all(feature = "web", target_arch = "wasm32"),
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
)))]
fn load(_: &str) -> Option<String> {
    None
}
#[cfg(not(any(
    all(feature = "web", target_arch = "wasm32"),
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
)))]
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
}
