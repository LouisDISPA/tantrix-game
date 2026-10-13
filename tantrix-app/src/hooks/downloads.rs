//! Root-owned browser installation state survives route changes.

use dioxus::prelude::*;

use crate::state::downloads::{Device, DownloadContext, InstallStatus};

pub fn use_download_context() {
    let device = use_signal(Device::default);
    let install_status = use_signal(InstallStatus::default);
    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    let prompt = use_signal(|| None);
    let _ctx = use_context_provider(move || DownloadContext {
        device,
        install_status,
        #[cfg(all(target_arch = "wasm32", feature = "web"))]
        prompt,
    });

    #[cfg(all(target_arch = "wasm32", feature = "web"))]
    {
        use crate::state::downloads::browser;
        // The hook owns the listeners; their Drop removes them before the
        // root's signals are disposed. Initial state changes happen in an effect.
        use_hook(move || std::rc::Rc::new(browser::InstallListeners::new(_ctx)));
        use_effect(move || browser::detect_device(_ctx));
    }
}
