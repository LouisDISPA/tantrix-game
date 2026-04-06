use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Theme::Light => "🌙",
            Theme::Dark => "☀",
        }
    }
}

pub static THEME: GlobalSignal<Theme> = Signal::global(|| Theme::Dark);

pub fn toggle_theme() {
    let current = *THEME.read();
    *THEME.write() = current.toggle();
}
