//! A small static web server for trying web builds locally.
//!
//! Browsers do not run WebAssembly games from `file://`, so `ptk serve`
//! (and the app's "Play in browser") serve the folder on `127.0.0.1`. Every
//! response carries the cross-origin isolation headers (COOP/COEP), which
//! multi-threaded builds need for `SharedArrayBuffer`. Byte ranges are
//! supported, so audio and video can seek. Only GET and HEAD, only files
//! inside the folder, and only this computer can connect.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::error::{Result, io_ctx};

/// A running server. Dropping it (or calling [`Server::stop`]) stops it.
pub struct Server {
    pub addr: SocketAddr,
    pub root: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    pub fn stop(&self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            // Wake the accept loop.
            let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Serves `root` on 127.0.0.1:`port` (0 picks a free port).
pub fn start(root: &Path, port: u16) -> Result<Server> {
    let root = root
        .canonicalize()
        .map_err(io_ctx(root.display().to_string()))?;
    let listener =
        TcpListener::bind(("127.0.0.1", port)).map_err(io_ctx(format!("port {port}")))?;
    let addr = listener.local_addr().map_err(io_ctx("listener address"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let (r, s) = (root.clone(), stop.clone());
    thread::spawn(move || {
        for conn in listener.incoming() {
            if s.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(stream) = conn {
                let r = r.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &r);
                });
            }
        }
    });
    Ok(Server { addr, root, stop })
}

pub fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("ogg" | "oga") => "audio/ogg",
        Some("m4a") => "audio/mp4",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
            && let Ok(b) = u8::from_str_radix(&path[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The file for a request path, if it is inside `root`.
pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let path = decode(url_path.split(['?', '#']).next().unwrap_or("/"));
    let mut out = root.to_owned();
    for c in Path::new(path.trim_start_matches('/')).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.is_dir() {
        out.push("index.html");
    }
    let real = out.canonicalize().ok()?;
    (real.starts_with(root) && real.is_file()).then_some(real)
}

fn handle(stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
        let mut range = None;
        let mut close = false;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
                break;
            }
            let (k, v) = h.split_once(':').unwrap_or((&h, ""));
            match k.trim().to_ascii_lowercase().as_str() {
                "range" => range = Some(v.trim().to_owned()),
                "connection" if v.trim().eq_ignore_ascii_case("close") => close = true,
                _ => {}
            }
        }
        let head = method == "HEAD";
        if method != "GET" && !head {
            respond(
                &mut out,
                405,
                "text/plain",
                b"Method not allowed",
                &[],
                head,
            )?;
        } else if let Some(file) = resolve(root, target) {
            send_file(&mut out, &file, range.as_deref(), head)?;
        } else {
            respond(&mut out, 404, "text/plain", b"Not found", &[], head)?;
        }
        if close {
            return Ok(());
        }
    }
}

fn parse_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    let spec = spec.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if len == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        return (n > 0).then(|| (len.saturating_sub(n), len - 1));
    }
    let start: u64 = a.parse().ok()?;
    let end = if b.is_empty() {
        len - 1
    } else {
        b.parse::<u64>().ok()?.min(len - 1)
    };
    (start <= end && start < len).then_some((start, end))
}

fn send_file(
    out: &mut TcpStream,
    path: &Path,
    range: Option<&str>,
    head: bool,
) -> std::io::Result<()> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let (status, start, end) = match range.and_then(|r| parse_range(r, len)) {
        Some((s, e)) => (206, s, e),
        None if range.is_some() && len > 0 => {
            let cr = format!("bytes */{len}");
            return respond(out, 416, "text/plain", b"", &[("Content-Range", &cr)], head);
        }
        None => (200, 0, len.saturating_sub(1)),
    };
    let count = if len == 0 { 0 } else { end - start + 1 };
    let cr = format!("bytes {start}-{end}/{len}");
    let mut extra: Vec<(&str, &str)> = vec![("Accept-Ranges", "bytes")];
    if status == 206 {
        extra.push(("Content-Range", &cr));
    }
    write_head(out, status, mime(path), count, &extra)?;
    if !head && count > 0 {
        f.seek(SeekFrom::Start(start))?;
        std::io::copy(&mut f.take(count), out)?;
    }
    out.flush()
}

fn write_head(
    out: &mut TcpStream,
    status: u16,
    content_type: &str,
    len: u64,
    extra: &[(&str, &str)],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        _ => "Error",
    };
    let mut h = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: require-corp\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\nCache-Control: no-cache\r\n"
    );
    for (k, v) in extra {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    out.write_all(h.as_bytes())
}

fn respond(
    out: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    extra: &[(&str, &str)],
    head: bool,
) -> std::io::Result<()> {
    write_head(out, status, content_type, body.len() as u64, extra)?;
    if !head {
        out.write_all(body)?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(addr: SocketAddr, path: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let mut s = TcpStream::connect(addr).unwrap();
        let r = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: x\r\n{r}Connection: close\r\n\r\n"
        )
        .unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).unwrap();
        let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (
            String::from_utf8_lossy(&buf[..split]).into_owned(),
            buf[split + 4..].to_vec(),
        )
    }

    #[test]
    fn serves_files_ranges_and_isolation_headers() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("index.html"), "<p>hi</p>").unwrap();
        std::fs::create_dir(tmp.path().join("data")).unwrap();
        std::fs::write(tmp.path().join("data/my file.wasm"), b"0123456789").unwrap();
        std::fs::write(tmp.path().parent().unwrap().join("secret.txt"), "no").ok();
        let server = start(tmp.path(), 0).unwrap();
        let (h, body) = get(server.addr, "/", None);
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        assert!(h.contains("Cross-Origin-Embedder-Policy: require-corp"));
        assert_eq!(body, b"<p>hi</p>");
        let (h, body) = get(server.addr, "/data/my%20file.wasm", Some("bytes=2-4"));
        assert!(
            h.starts_with("HTTP/1.1 206") && h.contains("application/wasm"),
            "{h}"
        );
        assert!(h.contains("Content-Range: bytes 2-4/10"));
        assert_eq!(body, b"234");
        assert!(get(server.addr, "/../secret.txt", None).0.contains("404"));
        assert!(
            get(server.addr, "/%2e%2e/secret.txt", None)
                .0
                .contains("404")
        );
        assert!(get(server.addr, "/missing.js", None).0.contains("404"));
        server.stop();
    }

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_range("bytes=0-", 10), Some((0, 9)));
        assert_eq!(parse_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(parse_range("bytes=5-100", 10), Some((5, 9)));
        assert_eq!(parse_range("bytes=10-", 10), None);
        assert_eq!(parse_range("items=0-1", 10), None);
    }
}
