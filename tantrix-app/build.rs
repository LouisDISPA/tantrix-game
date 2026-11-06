//! Names for the platform `cfg` predicates the app repeats, such as
//! `#[cfg(native_client)]`. Use these instead of spelling the predicates out.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    cfg_aliases::cfg_aliases! {
        // The desktop or Android app, whatever the compile target.
        native_client: { any(feature = "desktop", feature = "android") },
        // That app built for its own OS, where reqwest, the native WebSocket
        // and the settings file are available.
        native_io: {
            any(
                all(feature = "desktop", not(target_arch = "wasm32")),
                all(feature = "android", target_os = "android")
            )
        },
        // The browser app, with web-sys APIs.
        web_client: { all(target_arch = "wasm32", feature = "web") },
    }
}
