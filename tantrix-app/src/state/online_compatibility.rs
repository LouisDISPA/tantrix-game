//! A small preflight check for the online menu, independent of room messages.

use tantrix_proto::{OnlineVersion, ONLINE_VERSION, ONLINE_VERSION_PATH};

pub const DOWNLOAD_URL: &str = "https://tantrix.louis-dispa.fr/download";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    AppOutdated,
    ServerOutdated,
}

impl Compatibility {
    pub fn of(version: OnlineVersion) -> Self {
        match version.online_version.cmp(&ONLINE_VERSION) {
            std::cmp::Ordering::Equal => Self::Compatible,
            std::cmp::Ordering::Greater => Self::AppOutdated,
            std::cmp::Ordering::Less => Self::ServerOutdated,
        }
    }
}

/// Use the same origin as the room transport, including desktop settings and
/// the browser's optional TANTRIX_ROOM_URL override.
pub fn server_origin() -> String {
    super::room_connection::room_url("", false)
        .trim_end_matches("/room/")
        .replacen("ws", "http", 1)
}

pub async fn check(server: &str) -> Result<Compatibility, ()> {
    let url = format!("{}{ONLINE_VERSION_PATH}", server.trim_end_matches('/'));
    let body = fetch(&url).await?;
    let version = tantrix_proto::decode::<OnlineVersion>(body.as_bytes()).map_err(|_| ())?;
    Ok(Compatibility::of(version))
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
async fn fetch(url: &str) -> Result<String, ()> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|_| ())?
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?
        .text()
        .await
        .map_err(|_| ())
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
async fn fetch(url: &str) -> Result<String, ()> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{AbortSignal, Request, RequestInit, Response};

    let window = web_sys::window().ok_or(())?;
    let init = RequestInit::new();
    init.set_signal(Some(&AbortSignal::timeout_with_u32(8000)));
    let request = Request::new_with_str_and_init(url, &init).map_err(|_| ())?;
    request
        .headers()
        .set("Cache-Control", "no-cache")
        .map_err(|_| ())?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| ())?
        .dyn_into()
        .map_err(|_| ())?;
    if !response.ok() {
        return Err(());
    }
    JsFuture::from(response.text().map_err(|_| ())?)
        .await
        .map_err(|_| ())?
        .as_string()
        .ok_or(())
}

#[cfg(not(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "web", target_arch = "wasm32")
)))]
async fn fetch(_url: &str) -> Result<String, ()> {
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_is_independent_of_release_versions() {
        for (version, expected) in [
            (ONLINE_VERSION, Compatibility::Compatible),
            (ONLINE_VERSION + 1, Compatibility::AppOutdated),
            (ONLINE_VERSION - 1, Compatibility::ServerOutdated),
        ] {
            assert_eq!(
                Compatibility::of(OnlineVersion {
                    online_version: version
                }),
                expected
            );
        }
        assert!(tantrix_proto::decode::<OnlineVersion>(br#"{"version":"0.1.2"}"#).is_err());
    }
}
