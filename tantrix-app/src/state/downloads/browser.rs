//! Browser API bindings only. Rendering and platform decisions stay in Rust.

use std::cell::RefCell;

use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortSignal, Event, Headers, Request, RequestInit, Response, Window};

use super::{Architecture, Device, DownloadContext, InstallStatus, Release};

use crate::state::releases::RELEASE_API;

#[wasm_bindgen]
extern "C" {
    // This nonstandard Chromium event is not exposed by web-sys.
    #[wasm_bindgen(extends = Event)]
    #[derive(Clone)]
    pub type InstallPrompt;

    #[wasm_bindgen(method, catch)]
    fn prompt(this: &InstallPrompt) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, getter, js_name = userChoice)]
    fn user_choice(this: &InstallPrompt) -> Promise;
}

pub struct InstallListeners {
    window: Window,
    before: Closure<dyn FnMut(Event)>,
    installed: Closure<dyn FnMut(Event)>,
}

impl InstallListeners {
    pub fn new(mut ctx: DownloadContext) -> Option<Self> {
        let window = web_sys::window()?;
        let before = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            event.prevent_default();
            ctx.prompt.set(Some(event.unchecked_into()));
            if *ctx.install_status.peek() != InstallStatus::Installed {
                ctx.install_status.set(InstallStatus::Available);
            }
        });
        let installed = Closure::<dyn FnMut(Event)>::new(move |_| {
            ctx.prompt.set(None);
            ctx.install_status.set(InstallStatus::Installed);
        });
        let listeners = Self {
            window,
            before,
            installed,
        };
        listeners
            .window
            .add_event_listener_with_callback(
                "beforeinstallprompt",
                listeners.before.as_ref().unchecked_ref(),
            )
            .ok()?;
        listeners
            .window
            .add_event_listener_with_callback(
                "appinstalled",
                listeners.installed.as_ref().unchecked_ref(),
            )
            .ok()?;
        Some(listeners)
    }
}

impl Drop for InstallListeners {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback(
            "beforeinstallprompt",
            self.before.as_ref().unchecked_ref(),
        );
        let _ = self.window.remove_event_listener_with_callback(
            "appinstalled",
            self.installed.as_ref().unchecked_ref(),
        );
    }
}

pub fn detect_device(mut ctx: DownloadContext) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let navigator = window.navigator();
    let device = Device::detect(
        &navigator.user_agent().unwrap_or_default(),
        navigator.max_touch_points(),
    );
    ctx.device.set(device);
    let standalone = window
        .match_media("(display-mode: standalone)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
        || property(navigator.as_ref(), "standalone")
            .ok()
            .and_then(|v| v.as_bool())
            == Some(true);
    if standalone {
        ctx.install_status.set(InstallStatus::Installed);
    }
    spawn(async move {
        // Safari and other browsers may not expose Client Hints. Missing or
        // rejected hints leave architecture unknown, so the download uses its default.
        if let Ok(architecture) = architecture(&navigator).await {
            ctx.device.set(Device {
                architecture,
                ..device
            });
        }
    });
}

async fn architecture(navigator: &web_sys::Navigator) -> Result<Option<Architecture>, JsValue> {
    let data = property(navigator.as_ref(), "userAgentData")?;
    let method: Function = property(&data, "getHighEntropyValues")?.dyn_into()?;
    let hints = Array::new();
    hints.push(&"architecture".into());
    hints.push(&"bitness".into());
    let promise: Promise = method.call1(&data, &hints)?.dyn_into()?;
    let values = JsFuture::from(promise).await?;
    Ok(match string(&values, "architecture")?.as_str() {
        "arm" => Some(Architecture::Arm64),
        "x86" if string(&values, "bitness")? == "64" => Some(Architecture::X64),
        _ => None,
    })
}

pub fn install(mut ctx: DownloadContext) {
    let Some(prompt) = ctx.prompt.take() else {
        return;
    };
    ctx.install_status.set(InstallStatus::Unavailable);
    // Call prompt synchronously in the click handler to preserve user
    // activation; only the browser's response is awaited.
    let Ok(promise) = prompt.prompt() else { return };
    // These signals are owned by the root and must survive leaving this page.
    spawn_forever(async move {
        let result = async {
            JsFuture::from(promise).await?;
            let choice = JsFuture::from(prompt.user_choice()).await?;
            string(&choice, "outcome")
        }
        .await;
        if result.as_deref() == Ok("accepted") {
            ctx.install_status.set(InstallStatus::Installed);
        }
    });
}

thread_local! {
    static RELEASE_CACHE: RefCell<Option<Release>> = const { RefCell::new(None) };
}

pub async fn fetch_release() -> Result<Release, ()> {
    if let Some(release) = RELEASE_CACHE.with(|cache| cache.borrow().clone()) {
        return Ok(release);
    }
    let release = request_release().await.map_err(|_| ())?;
    RELEASE_CACHE.with(|cache| *cache.borrow_mut() = Some(release.clone()));
    Ok(release)
}

async fn request_release() -> Result<Release, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No browser"))?;
    let init = RequestInit::new();
    init.set_signal(Some(&AbortSignal::timeout_with_u32(8000)));
    let headers = Headers::new()?;
    headers.set("Accept", "application/vnd.github+json")?;
    init.set_headers_headers(&headers);
    let request = Request::new_with_str_and_init(RELEASE_API, &init)?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str("Release unavailable"));
    }
    let body = JsFuture::from(response.text()?)
        .await?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Missing release body"))?;
    Release::parse(&body)
        .map_err(|error| JsValue::from_str(&error))?
        .filter(|release| !release.assets.is_empty())
        .ok_or_else(|| JsValue::from_str("No stable app downloads"))
}

fn property(value: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    Reflect::get(value, &name.into())
}

fn string(value: &JsValue, name: &str) -> Result<String, JsValue> {
    property(value, name)?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Missing string"))
}
