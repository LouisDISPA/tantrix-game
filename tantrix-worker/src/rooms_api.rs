//! Public room endpoints. Internal object operations are never public routes.
use std::net::IpAddr;

use crate::catalog::Owner;
use tantrix_proto::{is_game_id, CreateRoom, RoomApiError, RoomReference};
use tantrix_room::{lifecycle, rate_limit, session_cookie};
use worker::{Env, Method, Request, Response, Result};

/// Rate-limit bindings in `wrangler.toml`, configured to
/// [`rate_limit::ROOM_CREATION`] and [`rate_limit::CODE_LOOKUP`].
const CREATION_LIMIT: &str = "ROOM_CREATION_LIMIT";
const LOOKUP_LIMIT: &str = "CODE_LOOKUP_LIMIT";

pub fn error(error: RoomApiError) -> Result<Response> {
    let mut response = Response::from_json(&error)?.with_status(error.status());
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}

/// Whether the client may make one more request counted by `binding`.
async fn allowed(req: &Request, env: &Env, binding: &str) -> Result<bool> {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .and_then(|ip| ip.parse::<IpAddr>().ok());
    // Cloudflare always sets the header; without it everyone shares one count.
    let key = match ip.map(rate_limit::client_key) {
        Some(None) => return Ok(true),
        Some(Some(key)) => key.to_string(),
        None => "unknown".to_owned(),
    };
    Ok(env.rate_limiter(binding)?.limit(key).await?.success)
}

pub async fn handle(mut req: Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    if let Some(id) = path.strip_prefix("/room/") {
        if !is_game_id(id) {
            return error(RoomApiError::InvalidRequest);
        }
        return env
            .durable_object("ROOM")?
            .get_by_name(&format!("game:{id}"))?
            .fetch_with_request(req)
            .await;
    }
    let cookie = req.headers().get("Cookie")?;
    let (token, set_cookie) = session_cookie::identify(cookie.as_deref(), url.scheme() == "https");
    let result = if path == "/api/rooms" && req.method() == Method::Post {
        if !allowed(&req, env, CREATION_LIMIT).await? {
            return error(RoomApiError::RateLimited);
        }
        // Small JSON request only; reject oversized declared bodies early.
        if req
            .headers()
            .get("Content-Length")?
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n > 256)
        {
            return error(RoomApiError::InvalidRequest);
        }
        let body = req.text().await?;
        let request = if body.len() <= 256 {
            serde_json::from_str::<CreateRoom>(&body).ok()
        } else {
            None
        };
        let Some(request) = request.filter(|r| is_game_id(&r.request_id)) else {
            return error(RoomApiError::InvalidRequest);
        };
        let id = lifecycle::game_id(&token, &request.request_id);
        let request = Request::new(&format!("https://game/create/{id}"), Method::Post)?;
        env.durable_object("ROOM")?
            .get_by_name(&format!("game:{id}"))?
            .fetch_with_request(request)
            .await
    } else if let Some(code) = path
        .strip_prefix("/api/rooms/code/")
        .filter(|_| req.method() == Method::Get)
    {
        if !allowed(&req, env, LOOKUP_LIMIT).await? {
            return error(RoomApiError::RateLimited);
        }
        if !tantrix_proto::is_room_code(code) {
            return error(RoomApiError::InvalidRequest);
        }
        let mut owner = env
            .durable_object("ROOM_CODES")?
            .get_by_name(code)?
            .fetch_with_str("https://code/")
            .await?;
        let Some(owner) = owner.json::<Option<Owner>>().await? else {
            return error(RoomApiError::NotFound);
        };
        let mut response = env
            .durable_object("ROOM")?
            .get_by_name(&format!("game:{}", owner.id))?
            .fetch_with_str(&format!("https://game/info/{}?allocate=0", owner.id))
            .await?;
        if response.status_code() == 200 {
            let reference: RoomReference = response.json().await?;
            if reference.code.as_deref() != Some(code) {
                return error(RoomApiError::NotFound);
            }
            Response::from_json(&reference)
        } else {
            Ok(response)
        }
    } else if let Some(id) = path
        .strip_prefix("/api/rooms/")
        .filter(|_| req.method() == Method::Get)
    {
        if !is_game_id(id) {
            return error(RoomApiError::InvalidRequest);
        }
        env.durable_object("ROOM")?
            .get_by_name(&format!("game:{id}"))?
            .fetch_with_str(&format!("https://game/info/{id}?allocate=1"))
            .await
    } else {
        error(RoomApiError::InvalidRequest)
    };
    let response = result?;
    let headers = response.headers().clone();
    let mut response = response.with_headers(headers);
    response.headers_mut().set("Cache-Control", "no-store")?;
    if let Some(cookie) = set_cookie {
        response.headers_mut().set("Set-Cookie", &cookie)?;
    }
    Ok(response)
}
