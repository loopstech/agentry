//! A deliberately tiny HTTP/1.1 client for talking to the local app.
//!
//! The hook runs on every Claude Code event, so it avoids an HTTP library and
//! an async runtime. It only ever talks to 127.0.0.1, with `Connection: close`
//! and a `Content-Length` body, which is all the app's server sends back.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::TOKEN_HEADER;

/// How long to wait for the app to accept a connection. When the app isn't
/// running the port is closed and this fails immediately; the timeout only
/// matters if something odd is listening.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// Send a request to `127.0.0.1:<port>`. Any failure (refused, timeout,
/// malformed response) is `None`.
pub fn request(
    method: &str,
    port: u16,
    token: &str,
    path: &str,
    body: Option<&str>,
    read_timeout: Duration,
) -> Option<Response> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_nodelay(true).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .ok()?;
    stream.set_read_timeout(Some(read_timeout)).ok()?;

    let body = body.unwrap_or("");
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).ok()?;
    stream.write_all(body.as_bytes()).ok()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Option<Response> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;

    let mut content_length = None;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let name = name.trim().to_ascii_lowercase();
        if name == "content-length" {
            content_length = value.trim().parse::<usize>().ok();
        } else if name == "transfer-encoding" && value.trim().eq_ignore_ascii_case("chunked") {
            return None; // the app never sends chunked bodies
        }
    }
    let body = match content_length {
        Some(n) if n <= body.len() => &body[..n],
        Some(_) => return None, // truncated
        None => body,
    };
    Some(Response {
        status,
        body: String::from_utf8(body.to_vec()).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_response() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "{}");
    }

    #[test]
    fn rejects_truncated_and_garbage() {
        assert!(parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\n\r\n{}").is_none());
        assert!(parse_response(b"nonsense").is_none());
    }

    #[test]
    fn refused_connection_is_none_and_fast() {
        // Bind then drop to find a port nothing listens on.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let start = std::time::Instant::now();
        assert!(request("GET", port, "t", "/health", None, Duration::from_secs(1)).is_none());
        assert!(start.elapsed() < Duration::from_millis(400));
    }
}
