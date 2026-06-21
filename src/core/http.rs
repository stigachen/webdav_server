use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub reason: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, reason: &'static str) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn text(status: u16, reason: &'static str, text: impl Into<String>) -> Self {
        let body = text.into().into_bytes();
        Self::new(status, reason)
            .with_header("Content-Type", "text/plain; charset=utf-8")
            .with_body(body)
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }
}

pub fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    if reader.read_line(&mut first)? == 0 {
        return Ok(None);
    }
    let first = first.trim_end();
    let parts = first.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return Ok(None);
    }

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(Some(Request {
        method: parts[0].to_string(),
        path: parts[1].to_string(),
        headers,
        body,
    }))
}

pub fn write_response(
    stream: &mut TcpStream,
    request_method: &str,
    mut response: Response,
) -> std::io::Result<()> {
    response.headers.push((
        "Content-Length".to_string(),
        response.body.len().to_string(),
    ));
    response
        .headers
        .push(("Connection".to_string(), "close".to_string()));

    write!(
        stream,
        "HTTP/1.1 {} {}\r\n",
        response.status, response.reason
    )?;
    for (name, value) in response.headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")?;
    if request_method != "HEAD" {
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

pub fn percent_decode_path(input: &str) -> Result<String, String> {
    let path = input.split('?').next().unwrap_or(input);
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("Invalid percent encoding".to_string());
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| "Invalid percent encoding".to_string())?;
            out.push(
                u8::from_str_radix(hex, 16).map_err(|_| "Invalid percent encoding".to_string())?,
            );
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Path is not valid UTF-8".to_string())
}

pub fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::percent_decode_path;

    #[test]
    fn decodes_path_without_query() {
        assert_eq!(percent_decode_path("/a%20b.txt?x=1").unwrap(), "/a b.txt");
    }
}
