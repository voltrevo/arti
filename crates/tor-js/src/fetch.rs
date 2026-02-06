//! HTTP fetch implementation over arti-client DataStream
//!
//! This module implements HTTP/1.1 requests over Tor streams,
//! with TLS support via subtle-tls for HTTPS.

use crate::error::JsTorError;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use http::Method;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use url::Url;

/// HTTP response from a fetch request
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub url: Url,
}

impl HttpResponse {
    pub fn text(&self) -> Result<String, JsTorError> {
        String::from_utf8(self.body.clone())
            .map_err(|e| JsTorError::new("INVALID_UTF8", "parse", e.to_string(), false))
    }
}

/// Build an HTTP/1.1 request as raw bytes
pub fn build_http_request(
    url: &Url,
    method: &Method,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> Vec<u8> {
    let host = url.host_str().unwrap_or("localhost");
    let path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };

    let query = url.query().map(|q| format!("?{}", q)).unwrap_or_default();

    let mut request = format!(
        "{} {}{} HTTP/1.1\r\nHost: {}\r\n",
        method.as_str(),
        path,
        query,
        host
    );

    // Add default headers if not present
    if !headers.contains_key("User-Agent") && !headers.contains_key("user-agent") {
        request.push_str("User-Agent: tor-js/0.1.0\r\n");
    }
    if !headers.contains_key("Accept") && !headers.contains_key("accept") {
        request.push_str("Accept: */*\r\n");
    }
    if !headers.contains_key("Connection") && !headers.contains_key("connection") {
        request.push_str("Connection: close\r\n");
    }

    // Add custom headers
    for (key, value) in headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }

    // Add content-length for requests with body
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    // End headers
    request.push_str("\r\n");

    let mut bytes = request.into_bytes();

    // Add body if present
    if let Some(body) = body {
        bytes.extend_from_slice(body);
    }

    bytes
}

/// Execute an HTTP request over a stream and return the response bytes
pub async fn execute_http_request<S>(mut stream: S, request_bytes: &[u8]) -> Result<Vec<u8>, JsTorError>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    // Write the request
    stream
        .write_all(request_bytes)
        .await
        .map_err(|e| JsTorError::http_request(format!("Failed to write request: {}", e)))?;
    stream
        .flush()
        .await
        .map_err(|e| JsTorError::http_request(format!("Failed to flush request: {}", e)))?;

    // Read the response
    let mut response_bytes = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                response_bytes.extend_from_slice(&buf[..n]);
                debug!("Read {} bytes (total: {})", n, response_bytes.len());

                // Limit response size to 1MB for safety
                if response_bytes.len() > 1024 * 1024 {
                    warn!("Response exceeds 1MB limit, truncating");
                    break;
                }
            }
            Err(e) => {
                if response_bytes.is_empty() {
                    return Err(JsTorError::http_request(format!(
                        "Failed to read response: {}",
                        e
                    )));
                }
                // We have some data, maybe connection was closed
                debug!("Read ended with error (may be normal close): {}", e);
                break;
            }
        }
    }

    Ok(response_bytes)
}

/// Parse raw HTTP response bytes into HttpResponse
pub fn parse_http_response(data: &[u8], url: Url) -> Result<HttpResponse, JsTorError> {
    // Find the header/body separator
    let header_end = find_subsequence(data, b"\r\n\r\n")
        .ok_or_else(|| JsTorError::http_request("Invalid HTTP response: no header separator"))?;

    let header_bytes = &data[..header_end];
    let body = data[header_end + 4..].to_vec();

    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|e| JsTorError::http_request(format!("Invalid HTTP headers: {}", e)))?;

    let mut lines = header_str.lines();

    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = lines
        .next()
        .ok_or_else(|| JsTorError::http_request("Invalid HTTP response: no status line"))?;

    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(JsTorError::http_request("Invalid HTTP status line"));
    }

    let status: u16 = parts[1]
        .parse()
        .map_err(|e| JsTorError::http_request(format!("Invalid status code: {}", e)))?;

    // Parse headers
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    // Decode body based on Transfer-Encoding or Content-Length
    let mut decoded_body = body;

    let is_chunked = headers
        .get("transfer-encoding")
        .map(|te| te.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);

    if is_chunked {
        debug!("Decoding chunked transfer-encoding");
        decoded_body = decode_chunked_body(&decoded_body)
            .map_err(|e| JsTorError::http_request(format!("Failed to decode chunked body: {}", e)))?;
    } else if let Some(cl) = headers.get("content-length") {
        if let Ok(len) = cl.parse::<usize>() {
            if decoded_body.len() > len {
                debug!(
                    "Body longer than Content-Length ({} > {}), truncating",
                    decoded_body.len(),
                    len
                );
                decoded_body.truncate(len);
            }
        }
    }

    debug!(
        "Parsed response: status={}, headers={}, body_len={}",
        status,
        headers.len(),
        decoded_body.len()
    );

    Ok(HttpResponse {
        status,
        headers,
        body: decoded_body,
        url,
    })
}

/// Find the position of a subsequence in a byte slice
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Decode a chunked transfer-encoded body into plain bytes
fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut i = 0;

    loop {
        // Skip any leading whitespace/CRLF
        while i < body.len() && (body[i] == b'\r' || body[i] == b'\n' || body[i] == b' ') {
            i += 1;
        }

        if i >= body.len() {
            break;
        }

        // Find end of chunk-size line
        let line_start = i;
        let mut line_end_opt = None;
        while i + 1 < body.len() {
            if body[i] == b'\r' && body[i + 1] == b'\n' {
                line_end_opt = Some(i);
                break;
            }
            i += 1;
        }
        let line_end = match line_end_opt {
            Some(end) => end,
            None => {
                if !result.is_empty() {
                    debug!("Incomplete chunk size line, returning partial result");
                    break;
                }
                return Err("Incomplete chunk size line".into());
            }
        };

        let line = &body[line_start..line_end];

        // Parse hex size, ignoring any ";extensions"
        let size_str = match std::str::from_utf8(line) {
            Ok(s) => s.split(';').next().unwrap_or("").trim(),
            Err(_) => return Err("Chunk size line is not valid UTF-8".into()),
        };

        if size_str.is_empty() {
            i = line_end + 2;
            continue;
        }

        let size = match usize::from_str_radix(size_str, 16) {
            Ok(s) => s,
            Err(e) => {
                if !result.is_empty() {
                    debug!(
                        "Failed to parse chunk size '{}', returning partial result: {}",
                        size_str, e
                    );
                    break;
                }
                return Err(format!("Invalid chunk size '{}': {}", size_str, e));
            }
        };

        // Move past "\r\n"
        i = line_end + 2;

        // Size 0 means end of chunks
        if size == 0 {
            break;
        }

        // Ensure enough bytes for this chunk
        if i + size > body.len() {
            let available = body.len() - i;
            debug!(
                "Chunk extends beyond body length (need {}, have {}), taking available",
                size, available
            );
            result.extend_from_slice(&body[i..]);
            break;
        }

        // Copy chunk bytes
        result.extend_from_slice(&body[i..i + size]);
        i += size;

        // Each chunk is followed by "\r\n"
        if i + 1 < body.len() && body[i] == b'\r' && body[i + 1] == b'\n' {
            i += 2;
        }
    }

    Ok(result)
}

/// Perform an HTTP fetch over a Tor DataStream
pub async fn fetch<S>(
    stream: S,
    url: &Url,
    method: Method,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
    is_https: bool,
    host: &str,
) -> Result<HttpResponse, JsTorError>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + 'static,
{
    let request_bytes = build_http_request(url, &method, &headers, body.as_deref());
    debug!("Sending {} bytes of HTTP request", request_bytes.len());

    let response_bytes = if is_https {
        // Use subtle-tls for HTTPS
        use subtle_tls::{TlsConfig, TlsConnector};

        let config = TlsConfig {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            ..Default::default()
        };
        let connector = TlsConnector::with_config(config);

        let mut tls_stream = connector.connect(stream, host).await.map_err(|e| {
            JsTorError::tls(format!("TLS handshake failed with {}: {}", host, e))
        })?;
        info!("TLS 1.3 connection established with {} (WASM/SubtleCrypto)", host);

        execute_http_request(&mut tls_stream, &request_bytes).await?
    } else {
        execute_http_request(stream, &request_bytes).await?
    };

    info!("Received {} bytes of HTTP response", response_bytes.len());

    parse_http_response(&response_bytes, url.clone())
}
