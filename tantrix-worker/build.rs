//! Embeds the web client's `index.html` into the Worker.
//!
//! dx writes it next to the hashed JS/wasm bundle it references, so the client
//! must be built first (`just worker-client`). The SSR renderer splices each
//! rendered page into its `<div id="main">`.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-env-changed=TANTRIX_INDEX_HTML");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("index.html");

    // Native builds of the workspace compile this crate to an empty library.
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        fs::write(&out, "").unwrap();
        return;
    }

    let source = env::var("TANTRIX_INDEX_HTML").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../target/dx/tantrix-app/release/web/public/index.html")
    });
    println!("cargo::rerun-if-changed={}", source.display());

    let html = fs::read_to_string(&source).unwrap_or_else(|err| {
        panic!(
            "cannot read the client index.html at {}: {err}\n\
             Build the web client first: `just worker-client`",
            source.display()
        )
    });
    fs::write(&out, html).unwrap();
}
