//! Blocking HTTP daemon mode. See `Plans.md` (task 7.2).
//!
//! A tiny `std::net` HTTP/1.1 server — no async runtime, in keeping with the
//! crate's blocking ethos — that serves captures concurrently. Concurrency is
//! bounded by a [`Pool`] of permits (task 7.1): each request takes a permit for
//! the duration of its capture and returns `503` when all permits are in use.
//!
//! Request parsing, routing, and response building are pure and unit-tested;
//! [`serve`] is the thin accept loop.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use crate::pool::Pool;

/// A minimally-parsed HTTP request: method, path, and query parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
}

impl Request {
    /// The first value for query key `name`, if present.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Parse the request line (`GET /path?a=b HTTP/1.1`) and drain headers up to the
/// blank line. Returns `None` on a malformed/empty request line.
pub fn parse_request(reader: &mut impl BufRead) -> Option<Request> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target.to_string(), Vec::new()),
    };

    // Drain headers (we don't need them) until the blank line.
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    Some(Request {
        method,
        path,
        query,
    })
}

/// Parse `a=b&c=d` into pairs, percent-decoding `%XX` and `+`.
fn parse_query(raw: &str) -> Vec<(String, String)> {
    raw.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

/// Minimal `application/x-www-form-urlencoded` decode (`+` → space, `%XX` → byte).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Build an HTTP/1.1 response with `Connection: close`.
pub fn http_response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// Route a parsed request to a `(status, content_type, body)` response, running
/// captures via `capture(url, format)`.
///
/// - `GET /healthz` → `200 ok`
/// - `GET /capture?url=…&format=markdown|readable` → `200` text (`400` without
///   a URL, `502` on capture failure)
/// - other paths → `404`; non-GET → `405`
pub fn handle<F>(req: &Request, capture: &F) -> (u16, String, Vec<u8>)
where
    F: Fn(&str, &str) -> Result<String, String>,
{
    if req.method != "GET" {
        return (405, "text/plain".into(), b"method not allowed\n".to_vec());
    }
    match req.path.as_str() {
        "/healthz" => (200, "text/plain".into(), b"ok\n".to_vec()),
        "/capture" => {
            let Some(url) = req.query("url") else {
                return (400, "text/plain".into(), b"missing ?url\n".to_vec());
            };
            let format = req.query("format").unwrap_or("markdown");
            match capture(url, format) {
                Ok(body) => (200, "text/plain; charset=utf-8".into(), body.into_bytes()),
                Err(err) => (502, "text/plain".into(), format!("{err}\n").into_bytes()),
            }
        }
        _ => (404, "text/plain".into(), b"not found\n".to_vec()),
    }
}

/// Serve captures on `addr` until the listener errors. `capacity` bounds the
/// number of concurrent captures (a [`Pool`] of permits); excess requests get
/// `503`. `capture(url, format)` performs each capture.
pub fn serve<F>(addr: &str, capacity: usize, capture: F) -> std::io::Result<()>
where
    F: Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr)?;
    tracing::info!(%addr, capacity, "amber daemon listening");
    let pool = Arc::new(Pool::<()>::new(capacity));
    let capture = Arc::new(capture);
    for stream in listener.incoming() {
        let stream = stream?;
        let pool = Arc::clone(&pool);
        let capture = Arc::clone(&capture);
        std::thread::spawn(move || handle_conn(stream, &pool, capture.as_ref()));
    }
    Ok(())
}

/// Handle one connection: parse, take a concurrency permit (or `503`), capture,
/// and write the response. Public for hermetic socket tests.
pub fn handle_conn<F>(mut stream: TcpStream, pool: &Pool<()>, capture: &F)
where
    F: Fn(&str, &str) -> Result<String, String>,
{
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let Some(req) = parse_request(&mut reader) else {
        let _ = stream.write_all(&http_response(400, "text/plain", b"bad request\n"));
        return;
    };

    // A capture is the only expensive route; gate it on a concurrency permit.
    let response = if req.path == "/capture" {
        match pool.acquire(|| ()) {
            Some(()) => {
                let (status, ct, body) = handle(&req, capture);
                pool.release(());
                http_response(status, &ct, &body)
            }
            None => http_response(503, "text/plain", b"server busy\n"),
        }
    } else {
        let (status, ct, body) = handle(&req, capture);
        http_response(status, &ct, &body)
    };
    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    fn stub(url: &str, format: &str) -> Result<String, String> {
        if url == "bad" {
            Err("capture failed".to_string())
        } else {
            Ok(format!("[{format}] {url}"))
        }
    }

    fn parse(raw: &str) -> Request {
        parse_request(&mut Cursor::new(raw.as_bytes())).expect("parse")
    }

    #[test]
    fn parses_method_path_and_query() {
        let req = parse("GET /capture?url=https%3A%2F%2Fe.com%2F&format=readable HTTP/1.1\r\n\r\n");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/capture");
        assert_eq!(req.query("url"), Some("https://e.com/"));
        assert_eq!(req.query("format"), Some("readable"));
    }

    #[test]
    fn percent_and_plus_decoding() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("%2F%3A"), "/:");
    }

    #[test]
    fn http_response_is_well_formed() {
        let r = http_response(200, "text/plain", b"hi");
        let s = String::from_utf8(r).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Length: 2\r\n"));
        assert!(s.contains("Connection: close\r\n"));
        assert!(s.ends_with("\r\n\r\nhi"));
    }

    #[test]
    fn routes_healthz_capture_and_errors() {
        assert_eq!(
            handle(&parse("GET /healthz HTTP/1.1\r\n\r\n"), &stub).0,
            200
        );

        let (status, _, body) = handle(
            &parse("GET /capture?url=https://e.com&format=readable HTTP/1.1\r\n\r\n"),
            &stub,
        );
        assert_eq!(status, 200);
        assert_eq!(String::from_utf8(body).unwrap(), "[readable] https://e.com");

        // Missing url, capture error, unknown route, wrong method.
        assert_eq!(
            handle(&parse("GET /capture HTTP/1.1\r\n\r\n"), &stub).0,
            400
        );
        assert_eq!(
            handle(&parse("GET /capture?url=bad HTTP/1.1\r\n\r\n"), &stub).0,
            502
        );
        assert_eq!(handle(&parse("GET /nope HTTP/1.1\r\n\r\n"), &stub).0, 404);
        assert_eq!(
            handle(&parse("POST /capture HTTP/1.1\r\n\r\n"), &stub).0,
            405
        );
    }

    #[test]
    fn capture_defaults_to_markdown() {
        let (_, _, body) = handle(&parse("GET /capture?url=u HTTP/1.1\r\n\r\n"), &stub);
        assert_eq!(String::from_utf8(body).unwrap(), "[markdown] u");
    }

    /// End-to-end over a real loopback socket: handle_conn parses the request,
    /// takes a pool permit, captures via the stub, and writes the response.
    #[test]
    fn handle_conn_serves_over_a_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let pool = Pool::<()>::new(2);
            handle_conn(stream, &pool, &stub);
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /capture?url=https://e.com&format=markdown HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();

        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"), "response:\n{resp}");
        assert!(resp.contains("[markdown] https://e.com"));
        server.join().unwrap();
    }

    /// When the pool is exhausted, /capture returns 503 (concurrency bound, 7.1).
    #[test]
    fn capture_returns_503_when_pool_exhausted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let pool = Pool::<()>::new(1);
            // Hold the only permit (never released) so the request can't get one.
            assert!(pool.acquire(|| ()).is_some());
            handle_conn(stream, &pool, &stub);
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /capture?url=https://e.com HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        assert!(
            resp.starts_with("HTTP/1.1 503"),
            "expected 503, got:\n{resp}"
        );
        server.join().unwrap();
    }
}
