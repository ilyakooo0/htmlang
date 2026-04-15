use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const RELOAD_SCRIPT: &str =
    r#"<script>new EventSource("/_events").onmessage=()=>location.reload()</script>"#;

/// Serve a single HTML file (legacy single-file mode).
pub async fn run(port: u16, html_path: PathBuf, reload: broadcast::Sender<()>) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: failed to bind to port {}: {}", port, e);
            return;
        }
    };

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let path = html_path.clone();
            let rx = reload.subscribe();
            tokio::spawn(async move {
                let _ = handle(stream, path, None, rx).await;
            });
        }
    }
}

/// Serve a directory of HTML files with auto-compilation, directory index, and route mapping.
pub async fn run_dir(port: u16, root_dir: PathBuf, reload: broadcast::Sender<()>) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: failed to bind to port {}: {}", port, e);
            return;
        }
    };

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let dir = root_dir.clone();
            let rx = reload.subscribe();
            tokio::spawn(async move {
                let _ = handle_dir_request(stream, dir, rx).await;
            });
        }
    }
}

async fn handle_dir_request(
    mut stream: TcpStream,
    root_dir: PathBuf,
    reload_rx: broadcast::Receiver<()>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    if path == "/_events" {
        return handle_sse(stream, reload_rx).await;
    }

    if path.contains("..") {
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }

    let clean_path = path.trim_start_matches('/');

    // Try to resolve the request to a file
    let file_path = if clean_path.is_empty() {
        // Root: try index.html
        let index = root_dir.join("index.html");
        if index.exists() {
            Some(index)
        } else {
            // Generate directory listing
            let listing = generate_directory_listing(&root_dir);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
                listing.len(),
            );
            stream.write_all(header.as_bytes()).await?;
            stream.write_all(listing.as_bytes()).await?;
            return Ok(());
        }
    } else {
        let requested = root_dir.join(clean_path);
        if requested.is_file() {
            Some(requested)
        } else {
            // Try .html extension
            let with_html = requested.with_extension("html");
            if with_html.is_file() {
                Some(with_html)
            } else {
                // Try as directory with index.html
                let dir_index = requested.join("index.html");
                if dir_index.is_file() {
                    Some(dir_index)
                } else {
                    None
                }
            }
        }
    };

    match file_path {
        Some(fp) => {
            let body = match std::fs::read(&fp) {
                Ok(content) => {
                    if fp.extension().map_or(false, |e| e == "html") {
                        inject_reload_script(&content)
                    } else {
                        content
                    }
                }
                Err(_) => {
                    stream
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };
            let content_type = content_type_for(&fp);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
                content_type, body.len(),
            );
            stream.write_all(header.as_bytes()).await?;
            stream.write_all(&body).await?;
        }
        None => {
            let body = b"<h1>404 Not Found</h1>";
            let header = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).await?;
            stream.write_all(body).await?;
        }
    }

    Ok(())
}

/// Generate a directory listing HTML page showing available .html pages.
fn generate_directory_listing(dir: &Path) -> String {
    let mut pages = Vec::new();
    collect_html_pages(dir, dir, &mut pages);
    pages.sort();

    let mut html = String::from(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>htmlang dev server</title>\
         <style>*{margin:0;box-sizing:border-box}body{font-family:system-ui,-apple-system,sans-serif;padding:2rem;max-width:600px;margin:0 auto}\
         h1{margin-bottom:1.5rem;font-size:1.5rem;color:#333}a{display:block;padding:0.6rem 1rem;margin:0.25rem 0;color:#2563eb;\
         text-decoration:none;border-radius:6px;transition:background 0.15s}a:hover{background:#f0f4ff}\
         .path{color:#888;font-size:0.85rem}</style></head><body>\
         <h1>htmlang dev server</h1>",
    );

    if pages.is_empty() {
        html.push_str("<p>No .html pages found. Compile some .hl files first.</p>");
    } else {
        for page in &pages {
            let display = page.trim_start_matches('/');
            let clean = display.strip_suffix("/index.html").unwrap_or(
                display.strip_suffix(".html").unwrap_or(display),
            );
            let label = if clean.is_empty() { "index" } else { clean };
            html.push_str(&format!(
                "<a href=\"{}\">{} <span class=\"path\">{}</span></a>",
                page, label, display
            ));
        }
    }

    html.push_str(RELOAD_SCRIPT);
    html.push_str("</body></html>");
    html
}

fn collect_html_pages(base: &Path, dir: &Path, pages: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_html_pages(base, &path, pages);
            } else if path.extension().map_or(false, |e| e == "html") {
                if let Ok(rel) = path.strip_prefix(base) {
                    pages.push(format!("/{}", rel.display()));
                }
            }
        }
    }
}

async fn handle(
    mut stream: TcpStream,
    html_path: PathBuf,
    root_dir: Option<&Path>,
    reload_rx: broadcast::Receiver<()>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    if path == "/_events" {
        handle_sse(stream, reload_rx).await
    } else {
        handle_file(stream, &html_path, root_dir, path).await
    }
}

async fn handle_sse(
    mut stream: TcpStream,
    mut rx: broadcast::Receiver<()>,
) -> std::io::Result<()> {
    let headers = "HTTP/1.1 200 OK\r\n\
                   Content-Type: text/event-stream\r\n\
                   Cache-Control: no-cache\r\n\
                   Connection: keep-alive\r\n\r\n";
    stream.write_all(headers.as_bytes()).await?;

    loop {
        match rx.recv().await {
            Ok(()) => {
                if stream.write_all(b"data: reload\n\n").await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

async fn handle_file(
    mut stream: TcpStream,
    html_path: &Path,
    root_dir: Option<&Path>,
    request_path: &str,
) -> std::io::Result<()> {
    if request_path.contains("..") {
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }

    let file_path = if request_path == "/" {
        html_path.to_path_buf()
    } else {
        let dir = root_dir
            .unwrap_or_else(|| html_path.parent().unwrap_or(Path::new(".")));
        dir.join(&request_path[1..])
    };

    let body = match std::fs::read(&file_path) {
        Ok(content) => {
            if file_path.extension().map_or(false, |e| e == "html") {
                inject_reload_script(&content)
            } else {
                content
            }
        }
        Err(_) => {
            stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await?;
            return Ok(());
        }
    };

    let content_type = content_type_for(&file_path);
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
        body.len(),
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;

    Ok(())
}

fn inject_reload_script(content: &[u8]) -> Vec<u8> {
    let html = String::from_utf8_lossy(content);
    match html.rfind("</body>") {
        Some(pos) => {
            let mut result = String::with_capacity(html.len() + RELOAD_SCRIPT.len() + 1);
            result.push_str(&html[..pos]);
            result.push_str(RELOAD_SCRIPT);
            result.push('\n');
            result.push_str(&html[pos..]);
            result.into_bytes()
        }
        None => {
            let mut result = content.to_vec();
            result.extend_from_slice(RELOAD_SCRIPT.as_bytes());
            result
        }
    }
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("xml") => "application/xml",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
