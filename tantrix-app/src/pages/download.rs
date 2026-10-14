use dioxus::prelude::*;

use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;
use crate::state::downloads::{DownloadContext, InstallStatus, Platform, Release, RELEASES_URL};

#[component]
pub fn Download() -> Element {
    let nav = navigator();
    let mut mounted = use_signal(|| false);
    let release: Resource<Option<Result<Release, ()>>> = use_resource(move || async move {
        // SSR and the first browser render share the same compact skeleton.
        if !mounted() {
            return None;
        }
        #[cfg(all(target_arch = "wasm32", feature = "web"))]
        return Some(crate::state::downloads::browser::fetch_release().await);
        #[cfg(not(all(target_arch = "wasm32", feature = "web")))]
        None
    });

    rsx! {
        MenuScreen { scroll: true,
            MenuCard {
                title: "Get Tantrix",
                subtitle: "Play on your device",
                back_disabled: !mounted(),
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                main {
                    class: "download-content",
                    inert: (!mounted()).then_some(true),
                    onmounted: move |_| mounted.set(true),
                    NativeDownloads { release }
                    nav {
                        class: "download-release-links",
                        aria_label: "Release information",
                        a {
                            class: "button button-ghost button-link",
                            href: RELEASES_URL,
                            "Releases & changelog ↗"
                        }
                    }
                    WebInstall { ready: mounted }
                }
            }
        }
    }
}

#[component]
fn NativeDownloads(release: Resource<Option<Result<Release, ()>>>) -> Element {
    let data = use_memo(move || match &*release.read() {
        Some(Some(Ok(data))) => Some(data.clone()),
        _ => None,
    });
    let loading = use_memo(move || !matches!(&*release.read(), Some(Some(_))));

    rsx! {
        section {
            class: "download-section",
            "data-native": "",
            aria_busy: loading(),
            div { class: "download-heading",
                h2 { "Desktop app" }
                match &*release.read() {
                    Some(Some(Ok(data))) => rsx! {
                        span { class: "download-meta", "v{data.version}" }
                    },
                    Some(Some(Err(()))) => rsx! {
                        span { class: "download-meta", "Unavailable" }
                    },
                    _ => rsx! {
                        span { class: "download-skeleton download-skeleton-version", aria_hidden: "true" }
                    },
                }
            }
            ReleaseDownloads { release: data, loading }
        }
    }
}

#[component]
fn ReleaseDownloads(release: ReadSignal<Option<Release>>, loading: ReadSignal<bool>) -> Element {
    let ctx = use_context::<DownloadContext>();
    let choices = use_memo(move || {
        release
            .read()
            .as_ref()
            .map(|release| release.choices((ctx.device)()))
            .unwrap_or_default()
    });
    // Explicit choices take precedence over late architecture hints.
    let mut selected = use_signal(|| None::<String>);
    let selection = use_memo(move || {
        selected().unwrap_or_else(|| {
            Release::preferred(&choices.read(), (ctx.device)()).unwrap_or_default()
        })
    });
    let asset = use_memo(move || {
        choices
            .read()
            .iter()
            .find(|asset| asset.name == selection())
            .cloned()
    });

    rsx! {
        div { class: "download-control-row",
            if loading() {
                div {
                    class: "download-skeleton download-skeleton-action",
                    aria_hidden: "true",
                }
            } else if let Some(asset) = asset() {
                a {
                    class: "button button-primary button-full",
                    "data-download": "",
                    href: "{asset.url}",
                    match asset.platform {
                        Platform::Mac => "Download for macOS",
                        Platform::Windows => "Download for Windows",
                        _ => "Download for Linux",
                    }
                }
            } else {
                p { role: "status",
                    if release.read().is_none() {
                        "Downloads are unavailable right now."
                    } else if choices.read().is_empty() {
                        "Not available for this device."
                    } else {
                        "Choose your device below."
                    }
                }
            }
        }
        div { class: "download-control-row",
            if loading() {
                div {
                    class: "download-skeleton download-skeleton-action",
                    aria_hidden: "true",
                }
            } else if choices.read().len() > 1 {
                select {
                    class: "field",
                    aria_label: "Download for your device",
                    value: "{selection}",
                    onchange: move |event| selected.set(Some(event.value())),
                    option {
                        value: "",
                        disabled: true,
                        selected: selection().is_empty(),
                        "Choose your device…"
                    }
                    for choice in choices.read().iter() {
                        option {
                            key: "{choice.name}",
                            value: "{choice.name}",
                            selected: choice.name == selection(),
                            "{choice.label()}"
                        }
                    }
                }
            } else if let Some(asset) = asset() {
                p { class: "download-meta", "{asset.label()} · {asset.size_mb()} MB" }
            } else {
                p { class: "download-meta", "Try the web app below." }
            }
        }
    }
}

#[component]
fn WebInstall(ready: ReadSignal<bool>) -> Element {
    let ctx = use_context::<DownloadContext>();

    rsx! {
        section { class: "download-section download-web", "data-web": "",
            h2 { "Web app" }
            div { class: "download-control-row",
                if !ready() {
                    div {
                        class: "download-skeleton download-skeleton-action",
                        aria_hidden: "true",
                    }
                } else if (ctx.install_status)() == InstallStatus::Available {
                    button {
                        r#type: "button",
                        class: "button button-secondary button-full",
                        onclick: move |_| ctx.install(),
                        "Install the web app"
                    }
                } else {
                    p { role: "status",
                        if (ctx.install_status)() == InstallStatus::Installed {
                            "Already installed."
                        } else if (ctx.device)().platform == Platform::Ios {
                            "Safari: Share → Add to Home Screen"
                        } else if (ctx.device)().platform == Platform::Mac && (ctx.device)().safari {
                            "Safari: File → Add to Dock"
                        } else {
                            "Install from your browser’s menu."
                        }
                    }
                }
            }
        }
    }
}
