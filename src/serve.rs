use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const RELOAD_SCRIPT: &str =
    r#"<script>new EventSource("/_events").onmessage=()=>location.reload()</script>"#;

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
                let _ = handle(stream, path, rx).await;
            });
        }
    }
}

async fn handle(
    mut stream: TcpStream,
    html_path: PathBuf,
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
        handle_file(stream, &html_path, path).await
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
        let dir = html_path.parent().unwrap_or(Path::new("."));
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
        _ => "application/octet-stream",
    }
}
