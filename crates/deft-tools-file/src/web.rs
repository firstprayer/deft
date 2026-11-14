//! Pictures a document links to on the web. This is the only place deft goes
//! online: plain GET requests over http or https, with no cookies or
//! credentials, a short timeout, a size limit and a cap on the whole run.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Limits for one run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Time for one picture, from connecting to the last byte.
    pub each: Duration,
    /// Time for every picture of the run together.
    pub total: Duration,
    /// Bytes of one picture.
    pub max_bytes: u64,
    /// Pictures fetched in one run; the rest are not tried.
    pub max_count: usize,
    /// Downloads at the same time.
    pub parallel: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            each: Duration::from_secs(10),
            total: Duration::from_secs(90),
            max_bytes: 20 * 1024 * 1024,
            max_count: 300,
            parallel: 6,
        }
    }
}

/// The address to fetch: `//host/path` is taken as https.
fn absolute(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("//") {
        Some(format!("https:{url}"))
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// The file extension for picture bytes, when they are a picture format a
/// document can hold.
pub fn picture_extension(bytes: &[u8]) -> Option<&'static str> {
    let head = &bytes[..bytes.len().min(1024)];
    Some(if head.starts_with(b"\x89PNG") {
        "png"
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if head.starts_with(b"GIF8") {
        "gif"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        "webp"
    } else if head.starts_with(b"BM") {
        "bmp"
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        "tif"
    } else if String::from_utf8_lossy(head)
        .to_ascii_lowercase()
        .contains("<svg")
    {
        "svg"
    } else {
        return None;
    })
}

/// The bytes of a `data:` address holding a picture in base64.
pub fn data_url(url: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let rest = url.trim().strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    if !head.to_ascii_lowercase().ends_with(";base64") {
        return None;
    }
    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim_end_matches('='))
        .or_else(|_| {
            base64::engine::general_purpose::STANDARD_NO_PAD.decode(data.trim_end_matches('='))
        })
        .ok()?;
    picture_extension(&bytes).map(|_| bytes)
}

fn fetch_one(agent: &ureq::Agent, url: &str, limits: &Limits) -> Result<Vec<u8>, String> {
    let address = absolute(url).ok_or("not a web address")?;
    let mut response = agent.get(&address).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => format!("HTTP {code}"),
        ureq::Error::Timeout(_) => "timed out".to_string(),
        other => other.to_string(),
    })?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limits.max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limits.max_bytes {
        return Err("larger than the size limit".into());
    }
    if picture_extension(&bytes).is_none() {
        return Err("not a picture".into());
    }
    Ok(bytes)
}

/// Download pictures, a few at a time, within `limits`. Every address gets
/// an answer: the bytes, or why there are none.
pub fn fetch_pictures(urls: &[String], limits: Limits) -> HashMap<String, Result<Vec<u8>, String>> {
    let mut unique: Vec<&String> = Vec::new();
    for u in urls {
        if !unique.contains(&u) {
            unique.push(u);
        }
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(limits.each))
        .max_redirects(3)
        .user_agent(format!(
            "deft/{} (+https://deftcli.com)",
            env!("CARGO_PKG_VERSION")
        ))
        .build();
    let agent: ureq::Agent = config.into();
    let start = Instant::now();
    let queue = Mutex::new(unique.iter().enumerate());
    let results = Mutex::new(HashMap::new());
    std::thread::scope(|s| {
        for _ in 0..limits.parallel.clamp(1, 16) {
            s.spawn(|| {
                loop {
                    let next = queue.lock().map(|mut q| q.next()).ok().flatten();
                    let Some((n, url)) = next else { break };
                    let result = if n >= limits.max_count {
                        Err("too many pictures in one document".to_string())
                    } else if start.elapsed() > limits.total {
                        Err("time limit for downloads reached".to_string())
                    } else {
                        fetch_one(&agent, url, &limits)
                    };
                    if let Ok(mut r) = results.lock() {
                        r.insert((*url).clone(), result);
                    }
                }
            });
        }
    });
    results.into_inner().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";

    /// A local server answering each request with the next of `answers`.
    fn serve(answers: Vec<(&'static str, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (status, body) in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    #[test]
    fn pictures_are_fetched_and_the_rest_say_why_not() {
        let base = serve(vec![
            ("200 OK", PNG.to_vec()),
            ("404 Not Found", vec![]),
            ("200 OK", b"<html>login</html>".to_vec()),
        ]);
        let limits = Limits {
            parallel: 1,
            ..Limits::default()
        };
        let urls: Vec<String> = ["a.png", "b.png", "c.png"]
            .iter()
            .map(|p| format!("{base}/{p}"))
            .collect();
        let got = fetch_pictures(&urls, limits);
        assert_eq!(got[&urls[0]].as_deref(), Ok(PNG));
        assert_eq!(got[&urls[1]], Err("HTTP 404".to_string()));
        assert_eq!(got[&urls[2]], Err("not a picture".to_string()));
    }

    #[test]
    fn unreachable_and_odd_addresses_fail_cleanly() {
        // A port nothing listens on.
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/x.png", l.local_addr().unwrap())
        };
        let urls = vec![closed.clone(), "ftp://example.com/x.png".to_string()];
        let got = fetch_pictures(&urls, Limits::default());
        assert!(got[&closed].is_err());
        assert_eq!(got[&urls[1]], Err("not a web address".to_string()));
        let limits = Limits {
            max_count: 0,
            ..Limits::default()
        };
        assert!(fetch_pictures(&urls[..1], limits)[&closed].is_err());
    }

    #[test]
    fn data_addresses_hold_pictures() {
        let url = format!("data:image/png;base64,{}", crate::base64_encode(PNG));
        assert_eq!(data_url(&url).as_deref(), Some(PNG));
        assert_eq!(data_url("data:text/plain;base64,aGVsbG8="), None);
        assert_eq!(
            absolute("//upload.wikimedia.org/a.png").as_deref(),
            Some("https://upload.wikimedia.org/a.png")
        );
    }
}
