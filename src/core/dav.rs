use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::core::fs_backend::{FileSystemBackend, FsEntry};
use crate::core::http::{Request, Response, xml_escape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

pub fn options() -> Response {
    Response::new(204, "No Content")
        .with_header("DAV", "1,2")
        .with_header(
            "Allow",
            "OPTIONS, PROPFIND, GET, HEAD, PUT, DELETE, MKCOL, COPY, MOVE",
        )
}

pub fn propfind(backend: &FileSystemBackend, request: &Request) -> io::Result<Response> {
    let depth = request
        .headers
        .get("depth")
        .map(String::as_str)
        .unwrap_or("1");
    let xml = build_propfind_xml(backend, &request.path, depth)?;
    Ok(Response::new(207, "Multi-Status")
        .with_header("Content-Type", "application/xml; charset=utf-8")
        .with_body(xml.into_bytes()))
}

pub fn get_or_head(backend: &FileSystemBackend, request: &Request) -> io::Result<Response> {
    let (path, metadata) = backend.metadata(&request.path)?;
    if metadata.is_dir() {
        return Ok(Response::text(
            403,
            "Forbidden",
            "Directory listing is available via PROPFIND",
        ));
    }

    let size = metadata.len();
    let range = request
        .headers
        .get("range")
        .and_then(|value| parse_range(value, size));
    let (status, reason, start, end) = if let Some(range) = range {
        (206, "Partial Content", range.start, range.end)
    } else {
        (200, "OK", 0, size.saturating_sub(1))
    };

    let mut body = Vec::new();
    if size > 0 {
        let mut file = fs::File::open(&path)?;
        let take_len = end.saturating_sub(start) + 1;
        std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(start))?;
        file.take(take_len).read_to_end(&mut body)?;
    }

    let mut response = Response::new(status, reason)
        .with_header("Content-Type", content_type(&path))
        .with_header("Accept-Ranges", "bytes")
        .with_header("Last-Modified", http_date(&metadata))
        .with_body(body);
    if status == 206 {
        response = response.with_header("Content-Range", format!("bytes {start}-{end}/{size}"));
    }
    Ok(response)
}

pub fn parse_range(header: &str, size: u64) -> Option<ByteRange> {
    let value = header.strip_prefix("bytes=")?;
    let (left, right) = value.split_once('-')?;
    if left.is_empty() {
        let suffix = right.parse::<u64>().ok()?;
        if suffix == 0 || size == 0 {
            return None;
        }
        return Some(ByteRange {
            start: size.saturating_sub(suffix),
            end: size - 1,
        });
    }
    let start = left.parse::<u64>().ok()?;
    let end = if right.is_empty() {
        size.checked_sub(1)?
    } else {
        right.parse::<u64>().ok()?.min(size.checked_sub(1)?)
    };
    if start > end || start >= size {
        return None;
    }
    Some(ByteRange { start, end })
}

fn build_propfind_xml(
    backend: &FileSystemBackend,
    request_path: &str,
    depth: &str,
) -> io::Result<String> {
    let (_, metadata) = backend.metadata(request_path)?;
    let mut responses = vec![render_response(request_path, &metadata)];
    if metadata.is_dir() && depth != "0" {
        for entry in backend.list(request_path)? {
            responses.push(render_child_response(request_path, &entry));
        }
    }
    Ok(format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
{}
</D:multistatus>"#,
        responses.join("\n")
    ))
}

fn render_child_response(parent: &str, entry: &FsEntry) -> String {
    let base = if parent.ends_with('/') {
        parent.to_string()
    } else {
        format!("{parent}/")
    };
    let href = format!("{}{}/", base, encode_path_segment(&entry.name));
    let href = if entry.metadata.is_dir() {
        href
    } else {
        href.trim_end_matches('/').to_string()
    };
    render_response_with_href(&href, &entry.metadata)
}

fn render_response(path: &str, metadata: &fs::Metadata) -> String {
    let href = if metadata.is_dir() && !path.ends_with('/') {
        format!("{path}/")
    } else {
        path.to_string()
    };
    render_response_with_href(&href, metadata)
}

fn render_response_with_href(href: &str, metadata: &fs::Metadata) -> String {
    let resource_type = if metadata.is_dir() {
        "<D:collection/>"
    } else {
        ""
    };
    let len = if metadata.is_dir() { 0 } else { metadata.len() };
    format!(
        r#"  <D:response>
    <D:href>{}</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype>{}</D:resourcetype>
        <D:getcontentlength>{}</D:getcontentlength>
        <D:getlastmodified>{}</D:getlastmodified>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>"#,
        xml_escape(href),
        resource_type,
        len,
        xml_escape(&http_date(metadata))
    )
}

fn encode_path_segment(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => vec![byte as char],
            b' ' => vec!['%', '2', '0'],
            other => format!("%{other:02X}").chars().collect(),
        })
        .collect()
}

fn http_date(metadata: &fs::Metadata) -> String {
    let seconds = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}")
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteRange, parse_range};

    #[test]
    fn parses_normal_range() {
        assert_eq!(
            parse_range("bytes=2-5", 10),
            Some(ByteRange { start: 2, end: 5 })
        );
    }

    #[test]
    fn parses_suffix_range() {
        assert_eq!(
            parse_range("bytes=-4", 10),
            Some(ByteRange { start: 6, end: 9 })
        );
    }
}
