//! Initial WebView configuration shared by mobile targets.

use dioxus::mobile::Config;

pub fn config() -> Config {
    #[cfg(all(feature = "android", target_os = "android"))]
    let background = match crate::platform::android::current_theme() {
        crate::state::theme::Theme::Light => (255, 255, 255, 255),
        crate::state::theme::Theme::Dark => (23, 23, 23, 255),
    };
    #[cfg(not(all(feature = "android", target_os = "android")))]
    let background = (18, 18, 18, 255);
    Config::new()
        .with_background_color(background)
        .with_custom_head(format!(
            "<style id=\"app-styles\">{}</style>",
            crate::APP_CSS,
        ))
}
