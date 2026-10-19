pub mod actions;
pub mod ai_service;
#[cfg(feature = "android")]
pub mod android_updates;
pub mod config;
#[cfg(feature = "desktop")]
pub mod desktop_updates;
#[cfg(not(any(feature = "desktop", feature = "native", feature = "android")))]
pub mod downloads;
pub mod game_driver;
pub mod game_state;
pub mod gesture;
pub mod nickname;
pub mod online_compatibility;
pub mod online_state;
pub mod room_connection;
#[cfg(any(feature = "desktop", feature = "android"))]
pub mod server_settings;
#[cfg(test)]
pub mod test_games;
pub mod theme;
pub mod tile_drag;
pub mod ui_state;

#[cfg(any(feature = "desktop", feature = "android"))]
pub mod native_settings;
