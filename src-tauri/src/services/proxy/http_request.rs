use std::collections::HashMap;

pub const DEFAULT_HEADER_LIMIT_BYTES: usize = 64 * 1024;
const MAX_CHUNK_EXTENSION_BYTES: usize = DEFAULT_HEADER_LIMIT_BYTES;
const FORWARDED_REQUEST_HEADERS: &[&str] = &[
    "accept",
    "content-type",
    "idempotency-key",
    "openai-organization",
    "openai-project",
    "openai-beta",
    "user-agent",
];

#[derive(Clone)]
pub struct ParsedRequest {
    pub method: String,
    pub path: String,
    pub target: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub fn parse_http_request<R: std::io::Read>(
    reader: &mut R,
    body_limit: usize,
) -> Result<ParsedRequest, String> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 4096];
    let mut header_end = None;
    while header_end.is_none() && buffer.len() < DEFAULT_HEADER_LIMIT_BYTES {
        let read = reader
            .read(&mut temp)
            .map_err(|error| format!("读取请求失败: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        header_end = find_header_end(&buffer);
    }

    let header_end = header_end.ok_or_else(|| "HTTP 请求头不完整".to_string())?;
    if header_end > DEFAULT_HEADER_LIMIT_BYTES {
        return Err("HTTP 请求头过大".to_string());
    }

    let mut headers_buffer = [httparse::EMPTY_HEADER; 128];
    let mut parsed = httparse::Request::new(&mut headers_buffer);
    let status = parsed
        .parse(&buffer[..header_end + 4])
        .map_err(|error| format!("解析 HTTP 请求失败: {error}"))?;
    if !status.is_complete() {
        return Err("HTTP 请求头不完整".to_string());
    }
    let method = parsed
        .method
        .ok_or_else(|| "缺少 HTTP method".to_string())?
        .to_uppercase();
    let target = parsed
        .path
        .ok_or_else(|| "缺少 HTTP path".to_string())?
        .to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();
    let headers = parsed
        .headers
        .iter()
        .filter(|header| !header.name.is_empty())
        .map(|header| {
            (
                header.name.trim().to_ascii_lowercase(),
                String::from_utf8_lossy(header.value).trim().to_string(),
            )
        })
        .collect::<HashMap<_, _>>();

    let body_start = header_end + 4;
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    {
        let mut chunked = buffer.get(body_start..).unwrap_or_default().to_vec();
        decode_chunked_body(reader, &mut chunked, body_limit)?
    } else {
        let content_length = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        if content_length > body_limit {
            return Err("HTTP 请求 body 过大".to_string());
        }
        let mut body = buffer.get(body_start..).unwrap_or_default().to_vec();
        if body.len() < content_length {
            let remaining = content_length - body.len();
            let mut tail = vec![0_u8; remaining];
            reader
                .read_exact(&mut tail)
                .map_err(|error| format!("读取请求 body 失败: {error}"))?;
            body.extend_from_slice(&tail);
        }
        body.truncate(content_length);
        body
    };

    Ok(ParsedRequest {
        method,
        path,
        target,
        headers,
        body,
    })
}

pub fn forwarded_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    FORWARDED_REQUEST_HEADERS
        .iter()
        .filter_map(|name| {
            headers
                .get(*name)
                .map(|value| ((*name).to_string(), value.clone()))
        })
        .collect()
}

fn decode_chunked_body<R: std::io::Read>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    body_limit: usize,
) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    let mut cursor = 0_usize;
    loop {
        let line_end = read_until_crlf(reader, buffer, &mut cursor)?;
        let line = std::str::from_utf8(&buffer[cursor..line_end])
            .map_err(|_| "chunk size 不是有效 UTF-8".to_string())?;
        let mut parts = line.splitn(2, ';');
        let size_text = parts.next().unwrap_or("").trim();
        if let Some(extension) = parts.next() {
            if extension.len() > MAX_CHUNK_EXTENSION_BYTES {
                return Err("chunk extension 过大".to_string());
            }
        }
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "chunk size 不是十六进制数字".to_string())?;
        cursor = line_end + 2;
        if size == 0 {
            read_chunk_trailers(reader, buffer, &mut cursor)?;
            return Ok(decoded);
        }
        if decoded.len().saturating_add(size) > body_limit {
            return Err("HTTP 请求 body 过大".to_string());
        }
        ensure_available(reader, buffer, cursor + size + 2)?;
        decoded.extend_from_slice(&buffer[cursor..cursor + size]);
        cursor += size;
        if buffer.get(cursor..cursor + 2) != Some(b"\r\n") {
            return Err("chunk body 缺少终止换行".to_string());
        }
        cursor += 2;
    }
}

fn read_chunk_trailers<R: std::io::Read>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    cursor: &mut usize,
) -> Result<(), String> {
    loop {
        let line_end = read_until_crlf(reader, buffer, cursor)?;
        if line_end == *cursor {
            *cursor += 2;
            return Ok(());
        }
        if line_end.saturating_sub(*cursor) > DEFAULT_HEADER_LIMIT_BYTES {
            return Err("chunk trailer 过大".to_string());
        }
        *cursor = line_end + 2;
    }
}

fn read_until_crlf<R: std::io::Read>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    cursor: &mut usize,
) -> Result<usize, String> {
    loop {
        if let Some(relative) = buffer[*cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
        {
            return Ok(*cursor + relative);
        }
        if buffer.len() > DEFAULT_HEADER_LIMIT_BYTES + buffer_limit_slack() {
            return Err("HTTP chunk 数据过大".to_string());
        }
        let mut temp = [0_u8; 4096];
        let read = reader
            .read(&mut temp)
            .map_err(|error| format!("读取 chunked body 失败: {error}"))?;
        if read == 0 {
            return Err("chunked body 不完整".to_string());
        }
        buffer.extend_from_slice(&temp[..read]);
    }
}

fn ensure_available<R: std::io::Read>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    size: usize,
) -> Result<(), String> {
    while buffer.len() < size {
        let mut temp = [0_u8; 4096];
        let read = reader
            .read(&mut temp)
            .map_err(|error| format!("读取 chunked body 失败: {error}"))?;
        if read == 0 {
            return Err("chunked body 不完整".to_string());
        }
        buffer.extend_from_slice(&temp[..read]);
    }
    Ok(())
}

fn buffer_limit_slack() -> usize {
    2 * 1024 * 1024 + 4096
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_parser_preserves_query_and_decodes_chunked_body() {
        let raw = b"POST /v1/responses?trace=1 HTTP/1.1\r\n\
Host: localhost\r\nTransfer-Encoding: chunked\r\n\r\n\
4\r\ntest\r\n0\r\n\r\n";
        let parsed = parse_http_request(&mut raw.as_slice(), 2 * 1024 * 1024).expect("request");
        assert_eq!(parsed.path, "/v1/responses");
        assert_eq!(parsed.target, "/v1/responses?trace=1");
        assert_eq!(parsed.body, b"test");
    }

    #[test]
    fn forwarding_keeps_safe_openai_headers_and_replaces_authorization() {
        let headers = forwarded_headers(&HashMap::from([
            ("authorization".into(), "Bearer client-key".into()),
            ("openai-organization".into(), "org_1".into()),
            ("openai-project".into(), "proj_1".into()),
            ("idempotency-key".into(), "idem_1".into()),
            ("connection".into(), "keep-alive".into()),
        ]));
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("connection"));
        assert_eq!(headers["openai-organization"], "org_1");
        assert_eq!(headers["openai-project"], "proj_1");
        assert_eq!(headers["idempotency-key"], "idem_1");
    }
}
