pub(crate) mod actions;
pub(crate) mod ai_service;
#[cfg(feature = "android")]
pub(crate) mod android_updates;
pub(crate) mod config;
#[cfg(feature = "desktop")]
pub(crate) mod desktop_updates;
#[cfg(not(native_client))]
pub(crate) mod downloads;
pub(crate) mod game_driver;
pub(crate) mod game_state;
pub(crate) mod gesture;
pub(crate) mod keyboard;
pub(crate) mod nickname;
pub(crate) mod online_compatibility;
pub(crate) mod online_state;
#[cfg(any(feature = "android", not(feature = "desktop")))]
pub(crate) mod releases;
pub(crate) mod room_api;
pub(crate) mod room_connection;
pub(crate) mod server_endpoints;
#[cfg(native_client)]
pub(crate) mod server_settings;
#[cfg(test)]
pub(crate) mod test_games;
pub(crate) mod theme;
pub(crate) mod tile_drag;
pub(crate) mod ui_state;

#[cfg(native_client)]
pub(crate) mod native_settings;
