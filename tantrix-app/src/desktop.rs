//! Desktop window chrome: native traffic lights on macOS, transparent
//! app-drawn controls on Windows and Linux.

use dioxus::desktop::{tao, Config};
use dioxus::prelude::*;

use crate::components::theme_toggle::ThemeToggle;
use crate::state::theme::{Theme, THEME};

const DESKTOP_CSS: &str = include_str!("../assets/desktop.css");

pub fn config() -> Config {
    let window = tao::window::WindowBuilder::new()
        .with_title("Tantrix")
        .with_visible(false)
        .with_theme(Some(tao::window::Theme::Dark));

    #[cfg(target_os = "macos")]
    let window = {
        use tao::platform::macos::WindowBuilderExtMacOS;
        window
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true)
    };

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let window = window.with_decorations(false);

    #[cfg(target_os = "windows")]
    let window = {
        use tao::platform::windows::WindowBuilderExtWindows;
        window.with_undecorated_shadow(true)
    };

    let config = Config::new()
        .with_window(window)
        .with_background_color((18, 18, 18, 255))
        // Parse styles before the app mounts, rather than queueing head edits.
        .with_custom_head(format!(
            "<style id=\"app-styles\">{}</style><style id=\"desktop-styles\">{DESKTOP_CSS}</style>",
            crate::APP_CSS,
        ));
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let config = config.with_menu(None);
    config
}

#[component]
pub(crate) fn DesktopChrome() -> Element {
    let window = dioxus::desktop::use_window();
    let mut fullscreen = use_signal(|| window.fullscreen().is_some());
    let mut maximized = use_signal(|| window.is_maximized());
    let mut focused = use_signal(|| window.is_focused());
    let custom = cfg!(any(target_os = "windows", target_os = "linux"));

    let theme_window = window.clone();
    use_effect(move || {
        theme_window.set_theme(Some(match *THEME.resolve().read() {
            Theme::Light => tao::window::Theme::Light,
            Theme::Dark => tao::window::Theme::Dark,
        }));
    });

    let event_window = window.clone();
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if let tao::event::Event::WindowEvent {
            window_id, event, ..
        } = event
        {
            if *window_id == event_window.id() {
                match event {
                    tao::event::WindowEvent::Resized(_) => {
                        let value = event_window.fullscreen().is_some();
                        if fullscreen() != value {
                            fullscreen.set(value);
                        }
                        let value = event_window.is_maximized();
                        if maximized() != value {
                            maximized.set(value);
                        }
                    }
                    tao::event::WindowEvent::Focused(value) => focused.set(*value),
                    _ => {}
                }
            }
        }
    });

    let show_window = window.clone();
    let drag_window = window.clone();
    let zoom_window = window.clone();
    let minimize_window = window.clone();
    let maximize_window = window.clone();
    let close_window = window.clone();
    rsx! {
        // Wait until the app is mounted to avoid flashing unstyled chrome.
        span { hidden: true, onmounted: move |_| show_window.set_visible(true) }
        if !fullscreen() {
            header {
                class: "desktop-titlebar",
                "data-platform": std::env::consts::OS,
                "data-custom": "{custom}",
                "data-focused": "{focused}",
                onmousedown: move |event| {
                    if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary)
                    {
                        drag_window.drag();
                    }
                },
                ondoubleclick: move |_| zoom_window.toggle_maximized(),
                div { class: "desktop-titlebar-brand",
                    img { src: crate::FAVICON, alt: "" }
                    span { "TANTRIX" }
                }
                div {
                    class: "desktop-window-controls",
                    // Button presses must never start a window drag or
                    // bubble a double-click into the header's maximize.
                    onmousedown: move |event| event.stop_propagation(),
                    ondoubleclick: move |event| event.stop_propagation(),
                    ThemeToggle { class: "desktop-theme-toggle" }
                    if custom {
                        button {
                            r#type: "button",
                            class: "desktop-window-button",
                            aria_label: "Minimize window",
                            title: "Minimize",
                            onclick: move |_| minimize_window.set_minimized(true),
                            svg { view_box: "0 0 16 16", "aria-hidden": "true",
                                path { d: "M3 8.5h10" }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "desktop-window-button",
                            aria_label: if maximized() { "Restore window" } else { "Maximize window" },
                            title: if maximized() { "Restore" } else { "Maximize" },
                            onclick: move |_| maximize_window.toggle_maximized(),
                            svg { view_box: "0 0 16 16", "aria-hidden": "true",
                                if maximized() {
                                    path { d: "M5.5 5.5v-3h8v8h-3 M2.5 5.5h8v8h-8z" }
                                } else {
                                    path { d: "M3.5 3.5h9v9h-9z" }
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "desktop-window-button desktop-window-close",
                            aria_label: "Close window",
                            title: "Close",
                            onclick: move |_| close_window.close(),
                            svg { view_box: "0 0 16 16", "aria-hidden": "true",
                                path { d: "m3.5 3.5 9 9 m0-9-9 9" }
                            }
                        }
                    }
                }
            }
            if custom && !maximized() {
                for (edge, direction) in [
                    ("n", tao::window::ResizeDirection::North),
                    ("s", tao::window::ResizeDirection::South),
                    ("e", tao::window::ResizeDirection::East),
                    ("w", tao::window::ResizeDirection::West),
                    ("ne", tao::window::ResizeDirection::NorthEast),
                    ("nw", tao::window::ResizeDirection::NorthWest),
                    ("se", tao::window::ResizeDirection::SouthEast),
                    ("sw", tao::window::ResizeDirection::SouthWest),
                ]
                {
                    div {
                        key: "{edge}",
                        class: "desktop-resize-edge",
                        "data-edge": edge,
                        aria_hidden: "true",
                        onmousedown: {
                            let window = window.clone();
                            move |event| {
                                if event.trigger_button()
                                    == Some(dioxus::html::input_data::MouseButton::Primary)
                                {
                                    event.prevent_default();
                                    if let Err(error) = window.drag_resize_window(direction) {
                                        log::warn!("Could not resize the desktop window: {error}");
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
