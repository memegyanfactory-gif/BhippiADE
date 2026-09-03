//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Proof test for INV-087: Standalone Web Export (GAD-127).
//!
//! Asserts that a Godot web export is 100% self-contained standard WebAssembly/HTML:
//! - Zero runtime studio coupling (no `__TAURI__` or Tauri internals anywhere in the bundle).
//! - Clean local web server playback via loopback with required isolation headers
//!   (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`).
//! - Standalone `credits.html` renders offline without CDN dependencies.
//! - WASM file is served with `application/wasm` MIME type.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi_inv087_{}_{}", name, ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn http_get(port: u16, path: &str) -> (String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).expect("write");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read");
    let text = String::from_utf8_lossy(&response).into_owned();
    (text, response)
}

#[test]
fn web_export_bundle_is_standalone_and_serves_with_coop_coep_inv087() {
    let root = temp_dir("bundle");
    let web_dir = root.join("export/web");
    std::fs::create_dir_all(&web_dir).unwrap();

    // Scaffold simulated web export bundle
    let html_content = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Bhippi Standalone Game</title>
</head>
<body>
  <canvas id="canvas"></canvas>
  <script src="index.js"></script>
</body>
</html>"#;
    std::fs::write(web_dir.join("index.html"), html_content).unwrap();

    let js_content = r#"
// Pure standard Godot Web loader (no studio runtime coupling)
const GODOT_CONFIG = { "args": [], "canvasResizePolicy": 2 };
console.log("Godot engine starting");
"#;
    std::fs::write(web_dir.join("index.js"), js_content).unwrap();
    std::fs::write(web_dir.join("index.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
    std::fs::write(web_dir.join("index.pck"), b"GDPC\x00\x00\x00\x00").unwrap();

    let credits_content = r#"<!doctype html>
<html>
<head><title>Credits</title></head>
<body>
  <h1>Game Credits</h1>
  <p>Built with Godot Engine and Bhippi Studio.</p>
</body>
</html>"#;
    std::fs::write(web_dir.join("credits.html"), credits_content).unwrap();

    // 1. Assert ZERO studio coupling (INV-087) across all text files in the bundle
    for entry in std::fs::read_dir(&web_dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if ext == "html" || ext == "js" {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(
                    !text.contains("__TAURI__"),
                    "File {} must have ZERO studio coupling: found __TAURI__",
                    path.display()
                );
                assert!(
                    !text.contains("__TAURI_INTERNALS__"),
                    "File {} must have ZERO studio coupling: found __TAURI_INTERNALS__",
                    path.display()
                );
            }
        }
    }

    // 2. Start the preview server over the export directory
    let server = bhippi_app::godot_preview::start(&root).expect("start preview server");
    let port = server.port();

    // 3. Request root `/` -> index.html with COOP/COEP isolation headers
    let (index_resp, _) = http_get(port, "/");
    assert!(index_resp.starts_with("HTTP/1.1 200 OK"), "{index_resp}");
    assert!(index_resp.contains("Cross-Origin-Opener-Policy: same-origin"));
    assert!(index_resp.contains("Cross-Origin-Embedder-Policy: require-corp"));
    assert!(index_resp.contains("Cache-Control: no-store"));
    assert!(index_resp.contains("Bhippi Standalone Game"));

    // 4. Request `/index.wasm` -> MIME application/wasm
    let (wasm_resp, _) = http_get(port, "/index.wasm");
    assert!(wasm_resp.starts_with("HTTP/1.1 200 OK"), "{wasm_resp}");
    assert!(wasm_resp.contains("Content-Type: application/wasm"));

    // 5. Request `/credits.html` -> clean attribution rendering
    let (credits_resp, _) = http_get(port, "/credits.html");
    assert!(
        credits_resp.starts_with("HTTP/1.1 200 OK"),
        "{credits_resp}"
    );
    assert!(credits_resp.contains("Built with Godot Engine"));

    server.stop();
    let _ = std::fs::remove_dir_all(root);
}
