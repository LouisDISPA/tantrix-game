pub mod about;
#[cfg(feature = "android")]
pub mod android_update;
#[cfg(feature = "desktop")]
pub mod desktop_update;
#[cfg(not(any(feature = "desktop", feature = "native", feature = "android")))]
pub mod download;
pub mod game;
pub mod learn;
pub mod local_game;
pub mod main_menu;
pub mod not_found;
pub mod online;
