use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    #[cfg(all(feature = "android", target_os = "android"))]
    pub(crate) fn from_android_ui_mode(ui_mode: i32) -> Self {
        // Configuration.UI_MODE_NIGHT_MASK and UI_MODE_NIGHT_YES.
        if ui_mode & 0x30 == 0x20 {
            Self::Dark
        } else {
            Self::Light
        }
    }

    pub(crate) fn toggle(&self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    pub(crate) fn icon(&self) -> &'static str {
        match self {
            Theme::Light => "🌙",
            Theme::Dark => "☀",
        }
    }
}

pub(crate) static THEME: GlobalStore<Theme> = Global::new(initial_theme);

fn initial_theme() -> Theme {
    #[cfg(all(feature = "android", target_os = "android"))]
    return crate::platform::android::current_theme();
    #[cfg(not(all(feature = "android", target_os = "android")))]
    Theme::Dark
}

pub(crate) fn toggle_theme() {
    let current = *THEME.resolve().read();
    let next = current.toggle();
    THEME.resolve().set(next);
    #[cfg(all(feature = "android", target_os = "android"))]
    crate::platform::android::set_theme(next);
}
