pub(crate) mod about;
#[cfg(feature = "android")]
pub(crate) mod android_update;
#[cfg(feature = "desktop")]
pub(crate) mod desktop_update;
#[cfg(not(native_client))]
pub(crate) mod download;
pub(crate) mod game;
pub(crate) mod learn;
pub(crate) mod local_game;
pub(crate) mod main_menu;
pub(crate) mod not_found;
pub(crate) mod online;
