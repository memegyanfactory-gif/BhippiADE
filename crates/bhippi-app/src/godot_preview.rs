//! A one-directory static file server for the Godot web export (ADR-0043 §5, GAD-081).
//!
//! Godot's web export is a `.wasm`, a `.pck` and a loader that fetches both. Tauri's
//! `asset:` protocol will not serve them with the headers the runtime insists on, and a
//! `file://` iframe cannot fetch siblings at all — so the Preview button needs a real HTTP
//! origin. This is the smallest thing that is one: `std::net::TcpListener` on an ephemeral
//! loopback port, a bounded thread per connection, and **only** the bytes under
//! `<project>/export/web/`.
//!
//! No crate is pulled in for it deliberately. A dependency that can serve a directory can
//! usually also serve a *different* directory, follow a symlink out of it or grow a
//! configuration surface; here the only reachable files are the ones whose canonical path
//! starts with the export root, which is a property this file can state in twenty lines and
//! prove in a test.
//!
//! Two headers are not optional. `Cross-Origin-Opener-Policy: same-origin` plus
//! `Cross-Origin-Embedder-Policy: require-corp` are what a threaded Godot web build checks
//! for before it will start; serving them costs nothing on a single-threaded build and means
//! the same server works when the preset changes. `Cache-Control: no-store` is what makes
//! "export again and hit reload" show the new build instead of the old one.

use crate::commands::AppError;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The sub-directory of a project the preview serves, and nothing above it.
pub const WEB_EXPORT_DIR: &str = "export/web";
/// The document served for `/`.
pub const INDEX_FILE: &str = "index.html";
/// How many connections may be in flight at once. A browser opens a handful for one page;
/// beyond this the listener answers 503 rather than spawning threads without limit.
pub const MAX_CONNECTIONS: usize = 16;
/// The largest request line + headers accepted. A Godot page sends kilobytes, never more.
pub const MAX_REQUEST_BYTES: usize = 8 * 1024;
/// How long one connection may sit idle before the server hangs up.
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// The MIME type for one file extension, or `application/octet-stream`.
///
/// `.wasm` is the one that actually matters: `WebAssembly.instantiateStreaming` refuses
/// anything that is not `application/wasm`, and the failure surfaces in the page as a blank
/// canvas rather than as an error about a header.
#[must_use]
pub fn mime_for(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "pck" => "application/octet-stream",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "json" => "application/json; charset=utf-8",
        "ogg" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "css" => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Resolve one request target under `root`, or `None` when it escapes.
///
/// Two defences, because either alone has a hole. The lexical pass refuses `..` and absolute
/// or prefixed components before anything touches the filesystem, which is what stops
/// `/../../.ssh/id_rsa` from ever being opened. The canonical pass then re-checks the real
/// path, which is what stops a **symlink** inside the export folder from pointing outside it.
#[must_use]
pub fn resolve_under(root: &Path, target: &str) -> Option<PathBuf> {
    let without_query = target.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_decode(without_query);
    let trimmed = decoded.trim_start_matches('/');
    let relative = if trimmed.is_empty() {
        INDEX_FILE.to_owned()
    } else {
        trimmed.replace('\\', "/")
    };
    let candidate = Path::new(&relative);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            // Anything else is a way out of the folder: `..`, a drive letter, a root, or a
            // UNC prefix. None of them can appear in a URL a Godot page generates.
            _ => return None,
        }
    }
    let joined = root.join(candidate);
    let canonical = std::fs::canonicalize(&joined).ok()?;
    let canonical_root = std::fs::canonicalize(root).ok()?;
    if !canonical.starts_with(&canonical_root) {
        return None;
    }
    canonical.is_file().then_some(canonical)
}

/// `%20` → ` `. Only what a URL path can legally carry; a malformed escape is left alone
/// rather than guessed at, because guessing is how `%2e%2e` becomes `..`.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A running preview. Dropping it does **not** stop the server — [`PreviewServer::stop`]
/// does, and the session store calls it — so a handle can be cloned into a status reply.
#[derive(Debug)]
pub struct PreviewServer {
    url: String,
    port: u16,
    root: PathBuf,
    running: Arc<AtomicBool>,
}

impl PreviewServer {
    /// The URL to point the Browser pane at.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The directory this server can reach, and nothing above it.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stop accepting. The accept loop wakes on its own connection and exits.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // The loop is blocked in `accept`; one connection to our own port unblocks it.
        let _ignored = std::net::TcpStream::connect(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            self.port,
        )));
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for PreviewServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Serve `<project_root>/export/web` on a loopback port nobody else has.
///
/// # Errors
/// Fails when the export folder is not there (the caller is meant to offer an export first)
/// or when no loopback port can be bound.
pub fn start(project_root: &Path) -> Result<PreviewServer, AppError> {
    let root = project_root.join(WEB_EXPORT_DIR);
    if !root.is_dir() {
        return Err(AppError {
            message: format!("{} has no web export yet", project_root.display()),
            hint: Some(
                "Run Export ▾ → Web first; Preview serves what the export wrote.".to_owned(),
            ),
        });
    }
    if !root.join(INDEX_FILE).is_file() {
        return Err(AppError {
            message: format!("{} has no index.html", root.display()),
            hint: Some(
                "The last web export did not finish. Export again and watch the Output log."
                    .to_owned(),
            ),
        });
    }
    // Port 0 asks the OS for a free one, which is the only way to avoid racing another
    // Bhippi window (or anything else) for a fixed number.
    let listener = TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
        .map_err(|error| AppError {
            message: format!("could not open a preview port: {error}"),
            hint: Some("Something is blocking loopback sockets; check the firewall.".to_owned()),
        })?;
    let port = listener
        .local_addr()
        .map_err(|error| AppError {
            message: format!("the preview port is unreadable: {error}"),
            hint: None,
        })?
        .port();

    let running = Arc::new(AtomicBool::new(true));
    let serve_root = root.clone();
    let loop_running = running.clone();
    std::thread::Builder::new()
        .name("bhippi-godot-preview".to_owned())
        .spawn(move || accept_loop(&listener, &serve_root, &loop_running))
        .map_err(|error| AppError {
            message: format!("could not start the preview server: {error}"),
            hint: None,
        })?;

    Ok(PreviewServer {
        url: format!("http://127.0.0.1:{port}/{INDEX_FILE}"),
        port,
        root,
        running,
    })
}

fn accept_loop(listener: &TcpListener, root: &Path, running: &Arc<AtomicBool>) {
    let live = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            let mut stream = stream;
            let _ignored = write_status(
                &mut stream,
                503,
                "Service Unavailable",
                b"busy",
                "text/plain; charset=utf-8",
                false,
            );
            continue;
        }
        live.fetch_add(1, Ordering::SeqCst);
        let root = root.to_path_buf();
        let owned = live.clone();
        let spawned = std::thread::Builder::new()
            .name("bhippi-godot-preview-conn".to_owned())
            .spawn(move || {
                let mut stream = stream;
                if let Err(error) = handle(&mut stream, &root) {
                    tracing::debug!(%error, "preview connection ended early");
                }
                close_gracefully(&mut stream);
                owned.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            live.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// End a connection so the client sees the response rather than a reset.
///
/// Windows sends RST — discarding whatever is still in flight — when a socket is closed with
/// unread bytes in its receive buffer. A browser that pipelines, or sends a body with a
/// request we answered without reading, hits that every time, and the symptom is a preview
/// that loads on one machine and shows `ERR_CONNECTION_RESET` on another. Half-closing and
/// then draining is what turns the abort into an ordinary FIN.
fn close_gracefully(stream: &mut TcpStream) {
    let _ignored = stream.shutdown(std::net::Shutdown::Write);
    let mut sink = [0u8; 1024];
    while let Ok(read) = stream.read(&mut sink) {
        if read == 0 {
            break;
        }
    }
}

fn handle(stream: &mut TcpStream, root: &Path) -> std::io::Result<()> {
    let _ignored = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ignored = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
    let (method, target) = match read_request(stream)? {
        Some(request) => request,
        None => return Ok(()),
    };
    let head_only = method == "HEAD";
    if method != "GET" && !head_only {
        return write_status(
            stream,
            405,
            "Method Not Allowed",
            b"only GET and HEAD",
            "text/plain; charset=utf-8",
            false,
        );
    }
    let Some(path) = resolve_under(root, &target) else {
        // A path that leaves the folder is refused as forbidden rather than missing: 404
        // would invite a caller to keep guessing, and there is nothing here to find.
        let escaped = target.contains("..") || target.contains("%2e") || target.contains("%2E");
        let (code, reason) = if escaped {
            (403, "Forbidden")
        } else {
            (404, "Not Found")
        };
        return write_status(
            stream,
            code,
            reason,
            reason.as_bytes(),
            "text/plain; charset=utf-8",
            false,
        );
    };
    let body = std::fs::read(&path)?;
    write_status(stream, 200, "OK", &body, mime_for(&path), head_only)
}

/// Read the request line and drain the headers. Returns `None` on an empty connection.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<(String, String)>> {
    let mut reader = BufReader::new(stream.try_clone()?).take(MAX_REQUEST_BYTES as u64);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    // Headers are read and thrown away: nothing here varies on them, and leaving them in
    // the socket makes the client wait for a response it will never get to send its body to.
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    Ok(Some((method, target)))
}

fn write_status(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    body: &[u8],
    content_type: &str,
    head_only: bool,
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\n\
         Cross-Origin-Resource-Policy: cross-origin\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        len = body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::{mime_for, resolve_under, start, INDEX_FILE, WEB_EXPORT_DIR};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::{Path, PathBuf};

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bhippi-preview-{name}-{}", std::process::id()));
        let _ignored = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn export_project(name: &str) -> PathBuf {
        let root = temp_dir(name);
        let web = root.join(WEB_EXPORT_DIR);
        std::fs::create_dir_all(&web).expect("web dir");
        std::fs::write(web.join(INDEX_FILE), "<html><body>game</body></html>").expect("index");
        std::fs::write(web.join("index.wasm"), [0u8, 97, 115, 109]).expect("wasm");
        std::fs::write(root.join("secret.txt"), "not yours").expect("secret");
        root
    }

    fn request(port: u16, line: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .write_all(format!("{line}\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .expect("write");
        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("read");
        String::from_utf8_lossy(&response).into_owned()
    }

    #[test]
    fn every_extension_a_godot_export_ships_has_the_type_the_browser_needs() {
        assert_eq!(
            mime_for(Path::new("a/index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            mime_for(Path::new("index.js")),
            "text/javascript; charset=utf-8"
        );
        // The one that is load-bearing: instantiateStreaming refuses anything else.
        assert_eq!(mime_for(Path::new("index.wasm")), "application/wasm");
        assert_eq!(mime_for(Path::new("index.pck")), "application/octet-stream");
        assert_eq!(mime_for(Path::new("icon.png")), "image/png");
        assert_eq!(mime_for(Path::new("icon.svg")), "image/svg+xml");
        assert_eq!(mime_for(Path::new("favicon.ico")), "image/x-icon");
        assert_eq!(
            mime_for(Path::new("a.json")),
            "application/json; charset=utf-8"
        );
        assert_eq!(mime_for(Path::new("a.ogg")), "audio/ogg");
        assert_eq!(mime_for(Path::new("a.mp3")), "audio/mpeg");
        assert_eq!(mime_for(Path::new("a.wav")), "audio/wav");
        assert_eq!(mime_for(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(mime_for(Path::new("a.exe")), "application/octet-stream");
        // Case does not decide the type: an export written by a different tool may shout.
        assert_eq!(mime_for(Path::new("INDEX.WASM")), "application/wasm");
    }

    #[test]
    fn a_path_that_leaves_the_export_folder_resolves_to_nothing() {
        let root = export_project("traversal");
        let web = root.join(WEB_EXPORT_DIR);
        assert!(resolve_under(&web, "/index.html").is_some());
        assert!(resolve_under(&web, "/").is_some(), "`/` is index.html");
        for escape in [
            "/../secret.txt",
            "/../../secret.txt",
            "/%2e%2e/secret.txt",
            "/subdir/../../secret.txt",
            "C:/Windows/System32/drivers/etc/hosts",
            "/nothing-here.wasm",
        ] {
            assert!(
                resolve_under(&web, escape).is_none(),
                "{escape} must not resolve"
            );
        }
        let _ignored = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_real_round_trip_serves_the_export_and_refuses_everything_else() {
        let root = export_project("roundtrip");
        let server = start(&root).expect("the server starts");
        assert!(server.url().starts_with("http://127.0.0.1:"));
        assert!(server.url().ends_with("/index.html"));

        let index = request(server.port(), "GET /index.html HTTP/1.1");
        assert!(index.starts_with("HTTP/1.1 200 OK"), "{index}");
        assert!(index.contains("Content-Type: text/html; charset=utf-8"));
        assert!(index.contains("Cross-Origin-Opener-Policy: same-origin"));
        assert!(index.contains("Cross-Origin-Embedder-Policy: require-corp"));
        assert!(index.contains("Cache-Control: no-store"));
        assert!(index.contains("game"));

        let root_doc = request(server.port(), "GET / HTTP/1.1");
        assert!(
            root_doc.contains("game"),
            "`/` serves index.html: {root_doc}"
        );

        let wasm = request(server.port(), "GET /index.wasm HTTP/1.1");
        assert!(wasm.contains("Content-Type: application/wasm"), "{wasm}");

        let head = request(server.port(), "HEAD /index.html HTTP/1.1");
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
        assert!(head.contains("Content-Length: "));
        assert!(!head.contains("<body>"), "HEAD sends no body: {head}");

        let escape = request(server.port(), "GET /../secret.txt HTTP/1.1");
        assert!(escape.starts_with("HTTP/1.1 403 Forbidden"), "{escape}");
        assert!(!escape.contains("not yours"));

        let missing = request(server.port(), "GET /nope.png HTTP/1.1");
        assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");

        let posted = request(server.port(), "POST /index.html HTTP/1.1");
        assert!(posted.starts_with("HTTP/1.1 405"), "{posted}");

        server.stop();
        let _ignored = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_with_no_export_is_a_typed_error_with_the_next_step() {
        let root = temp_dir("no-export");
        let error = start(&root).expect_err("no export folder");
        assert!(error.hint.is_some());
        assert!(error.hint.unwrap_or_default().contains("Export"));
        let _ignored = std::fs::remove_dir_all(&root);
    }
}
