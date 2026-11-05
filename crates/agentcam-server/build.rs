//! Make sure `ui-dist/` exists so the crate compiles before the web UI
//! has been built. A real build replaces the placeholder (`npm run build`
//! in `web/`).

use std::{fs, path::Path};

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui-dist");
    let index = dist.join("index.html");
    if !index.exists() {
        fs::create_dir_all(&dist).expect("create ui-dist");
        fs::write(
            &index,
            "<!doctype html><meta charset=utf-8><title>agentcam</title>\
             <body style=\"background:#0b0b0d;color:#e6e6e6;font-family:system-ui;padding:2rem\">\
             <p>Web UI not built. Run <code>npm run build</code> in <code>web/</code>, then rebuild.</p>",
        )
        .expect("write placeholder index.html");
    }
    println!("cargo:rerun-if-changed=ui-dist");
}
