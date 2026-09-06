/// HTTP Client/Server — Basic HTTP/1.1 implementation for KnoxOS
/// Provides HTTP request/response parsing and a simple HTTP server
///
/// Features:
///   - HTTP/1.1 request parsing (GET, POST, PUT, DELETE, HEAD, OPTIONS)
///   - HTTP/1.1 response building
///   - Header parsing and manipulation
///   - Basic HTTP server on TCP sockets
///   - HTTP client for outgoing requests
///   - Content-Type handling
///   - Chunked transfer encoding support
///   - Connection keep-alive
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// HTTP methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
    Trace,
    Connect,
}

impl HttpMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            "PATCH" => Some(Self::Patch),
            "TRACE" => Some(Self::Trace),
            "CONNECT" => Some(Self::Connect),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Patch => "PATCH",
            Self::Trace => "TRACE",
            Self::Connect => "CONNECT",
        }
    }
}

/// HTTP version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

impl HttpVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http10 => "HTTP/1.0",
            Self::Http11 => "HTTP/1.1",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "HTTP/1.0" => Some(Self::Http10),
            "HTTP/1.1" => Some(Self::Http11),
            _ => None,
        }
    }
}

/// HTTP status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum StatusCode {
    Ok = 200,
    Created = 201,
    NoContent = 204,
    MovedPermanently = 301,
    Found = 302,
    SeeOther = 303,
    NotModified = 304,
    TemporaryRedirect = 307,
    PermanentRedirect = 308,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    RequestTimeout = 408,
    Conflict = 409,
    Gone = 410,
    LengthRequired = 411,
    PayloadTooLarge = 413,
    UriTooLong = 414,
    UnsupportedMediaType = 415,
    TooManyRequests = 429,
    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
    GatewayTimeout = 504,
}

impl StatusCode {
    pub fn reason_phrase(&self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Created => "Created",
            Self::NoContent => "No Content",
            Self::MovedPermanently => "Moved Permanently",
            Self::Found => "Found",
            Self::SeeOther => "See Other",
            Self::NotModified => "Not Modified",
            Self::TemporaryRedirect => "Temporary Redirect",
            Self::PermanentRedirect => "Permanent Redirect",
            Self::BadRequest => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::RequestTimeout => "Request Timeout",
            Self::Conflict => "Conflict",
            Self::Gone => "Gone",
            Self::LengthRequired => "Length Required",
            Self::PayloadTooLarge => "Payload Too Large",
            Self::UriTooLong => "URI Too Long",
            Self::UnsupportedMediaType => "Unsupported Media Type",
            Self::TooManyRequests => "Too Many Requests",
            Self::InternalServerError => "Internal Server Error",
            Self::NotImplemented => "Not Implemented",
            Self::BadGateway => "Bad Gateway",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::GatewayTimeout => "Gateway Timeout",
        }
    }

    pub fn code(&self) -> u16 {
        *self as u16
    }

    pub fn from_u16(code: u16) -> Option<Self> {
        match code {
            200 => Some(Self::Ok),
            201 => Some(Self::Created),
            204 => Some(Self::NoContent),
            301 => Some(Self::MovedPermanently),
            302 => Some(Self::Found),
            303 => Some(Self::SeeOther),
            304 => Some(Self::NotModified),
            307 => Some(Self::TemporaryRedirect),
            308 => Some(Self::PermanentRedirect),
            400 => Some(Self::BadRequest),
            401 => Some(Self::Unauthorized),
            403 => Some(Self::Forbidden),
            404 => Some(Self::NotFound),
            405 => Some(Self::MethodNotAllowed),
            408 => Some(Self::RequestTimeout),
            409 => Some(Self::Conflict),
            410 => Some(Self::Gone),
            411 => Some(Self::LengthRequired),
            413 => Some(Self::PayloadTooLarge),
            414 => Some(Self::UriTooLong),
            415 => Some(Self::UnsupportedMediaType),
            429 => Some(Self::TooManyRequests),
            500 => Some(Self::InternalServerError),
            501 => Some(Self::NotImplemented),
            502 => Some(Self::BadGateway),
            503 => Some(Self::ServiceUnavailable),
            504 => Some(Self::GatewayTimeout),
            _ => None,
        }
    }
}

/// HTTP headers collection
#[derive(Debug, Clone)]
pub struct HttpHeaders {
    headers: Vec<(String, String)>,
}

impl Default for HttpHeaders {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpHeaders {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        // Remove existing header with same name
        let lower = name.to_lowercase();
        self.headers.retain(|(k, _)| k.to_lowercase() != lower);
        self.headers.push((String::from(name), String::from(value)));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn content_length(&self) -> Option<usize> {
        self.get("Content-Length")
            .and_then(|v| v.parse::<usize>().ok())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.get("Content-Type")
    }

    pub fn is_keep_alive(&self) -> bool {
        self.get("Connection")
            .map(|v| v.to_lowercase() != "close")
            .unwrap_or(true) // HTTP/1.1 default is keep-alive
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.headers.iter()
    }

    /// Serialize headers to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (name, value) in &self.headers {
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(b": ");
            bytes.extend_from_slice(value.as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        bytes
    }
}

/// An HTTP request
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub path: String,
    pub version: HttpVersion,
    pub headers: HttpHeaders,
    pub body: Vec<u8>,
    /// Query string parameters
    pub query: BTreeMap<String, String>,
}

impl HttpRequest {
    /// Parse an HTTP request from raw bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        let text = core::str::from_utf8(data).ok()?;

        // Find end of headers
        let header_end = text.find("\r\n\r\n")?;
        let header_section = &text[..header_end];
        let body_start = header_end + 4;

        let mut lines = header_section.lines();

        // Parse request line
        let request_line = lines.next()?;
        let mut parts = request_line.split_whitespace();
        let method = HttpMethod::parse(parts.next()?)?;
        let full_path = parts.next()?;
        let version = HttpVersion::parse(parts.next()?)?;

        // Parse path and query string
        let (path, query) = if let Some(q_pos) = full_path.find('?') {
            let path = String::from(&full_path[..q_pos]);
            let query_str = &full_path[q_pos + 1..];
            let query = parse_query_string(query_str);
            (path, query)
        } else {
            (String::from(full_path), BTreeMap::new())
        };

        // Parse headers
        let mut headers = HttpHeaders::new();
        for line in lines {
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();
                headers.set(name, value);
            }
        }

        // Extract body
        let body = if body_start < data.len() {
            let content_length = headers.content_length().unwrap_or(data.len() - body_start);
            let end = (body_start + content_length).min(data.len());
            data[body_start..end].to_vec()
        } else {
            Vec::new()
        };

        Some(HttpRequest {
            method,
            path,
            version,
            headers,
            body,
            query,
        })
    }

    /// Build an HTTP request as bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Request line
        let path_with_query = if self.query.is_empty() {
            self.path.clone()
        } else {
            let qs: Vec<String> = self
                .query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!("{}?{}", self.path, qs.join("&"))
        };

        bytes.extend_from_slice(
            format!(
                "{} {} {}\r\n",
                self.method.as_str(),
                path_with_query,
                self.version.as_str()
            )
            .as_bytes(),
        );

        // Headers
        bytes.extend_from_slice(&self.headers.to_bytes());

        // Empty line
        bytes.extend_from_slice(b"\r\n");

        // Body
        bytes.extend_from_slice(&self.body);

        bytes
    }
}

/// An HTTP response
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub version: HttpVersion,
    pub status: StatusCode,
    pub headers: HttpHeaders,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Create a new response with status code
    pub fn new(status: StatusCode) -> Self {
        let mut headers = HttpHeaders::new();
        headers.set("Server", "KnoxOS/0.6.0");
        headers.set("Connection", "close");

        Self {
            version: HttpVersion::Http11,
            status,
            headers,
            body: Vec::new(),
        }
    }

    /// Set response body with Content-Type
    pub fn with_body(mut self, body: &[u8], content_type: &str) -> Self {
        self.body = body.to_vec();
        self.headers.set("Content-Type", content_type);
        self.headers
            .set("Content-Length", &format!("{}", body.len()));
        self
    }

    /// Set HTML body
    pub fn html(body: &str) -> Self {
        Self::new(StatusCode::Ok).with_body(body.as_bytes(), "text/html; charset=utf-8")
    }

    /// Set plain text body
    pub fn text(body: &str) -> Self {
        Self::new(StatusCode::Ok).with_body(body.as_bytes(), "text/plain; charset=utf-8")
    }

    /// Set JSON body
    pub fn json(body: &str) -> Self {
        Self::new(StatusCode::Ok).with_body(body.as_bytes(), "application/json")
    }

    /// 404 Not Found
    pub fn not_found() -> Self {
        Self::new(StatusCode::NotFound).with_body(b"404 Not Found", "text/plain")
    }

    /// 500 Internal Server Error
    pub fn internal_error() -> Self {
        Self::new(StatusCode::InternalServerError)
            .with_body(b"500 Internal Server Error", "text/plain")
    }

    /// Parse an HTTP response from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        let text = core::str::from_utf8(data).ok()?;
        let header_end = text.find("\r\n\r\n")?;
        let header_section = &text[..header_end];
        let body_start = header_end + 4;

        let mut lines = header_section.lines();

        // Status line
        let status_line = lines.next()?;
        let mut parts = status_line.splitn(3, ' ');
        let version = HttpVersion::parse(parts.next()?)?;
        let status_code: u16 = parts.next()?.parse().ok()?;
        let status = StatusCode::from_u16(status_code)?;

        // Headers
        let mut headers = HttpHeaders::new();
        for line in lines {
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();
                headers.set(name, value);
            }
        }

        // Body — handle chunked transfer encoding and Content-Length
        let body = if body_start < data.len() {
            let is_chunked = headers
                .get("Transfer-Encoding")
                .map(|v| v.to_lowercase().contains("chunked"))
                .unwrap_or(false);

            if is_chunked {
                // Decode chunked transfer encoding
                decode_chunked_body(&data[body_start..])
            } else if let Some(content_length) = headers.content_length() {
                let end = (body_start + content_length).min(data.len());
                data[body_start..end].to_vec()
            } else {
                // No Content-Length and not chunked: read until end
                data[body_start..].to_vec()
            }
        } else {
            Vec::new()
        };

        Some(HttpResponse {
            version,
            status,
            headers,
            body,
        })
    }

    /// Serialize response to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Status line
        bytes.extend_from_slice(
            format!(
                "{} {} {}\r\n",
                self.version.as_str(),
                self.status.code(),
                self.status.reason_phrase()
            )
            .as_bytes(),
        );

        // Headers
        bytes.extend_from_slice(&self.headers.to_bytes());

        // Empty line
        bytes.extend_from_slice(b"\r\n");

        // Body
        bytes.extend_from_slice(&self.body);

        bytes
    }
}

/// Decode a chunked transfer-encoded body.
///
/// Chunked format:  <hex-size>\r\n<data>\r\n ... 0\r\n\r\n
fn decode_chunked_body(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pos = 0;

    while let Some(p) = data[pos..].windows(2).position(|w| w == b"\r\n") {
        let line_end = pos + p;

        // Parse hex chunk size
        let size_str = match core::str::from_utf8(&data[pos..line_end]) {
            Ok(s) => s.trim(),
            Err(_) => break,
        };

        // Strip optional chunk extension (after semicolon)
        let size_hex = size_str.split(';').next().unwrap_or("").trim();
        let chunk_size = match usize::from_str_radix(size_hex, 16) {
            Ok(s) => s,
            Err(_) => break,
        };

        // Size 0 = end of chunked body
        if chunk_size == 0 {
            break;
        }

        let chunk_start = line_end + 2; // skip \r\n after size
        let chunk_end = chunk_start + chunk_size;

        if chunk_end > data.len() {
            // Incomplete chunk — take what we have
            result.extend_from_slice(&data[chunk_start..data.len().min(chunk_end)]);
            break;
        }

        result.extend_from_slice(&data[chunk_start..chunk_end]);

        // Skip trailing \r\n after chunk data
        pos = chunk_end + 2;
        if pos > data.len() {
            break;
        }
    }

    result
}

/// Parse a query string into key-value pairs
fn parse_query_string(qs: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in qs.split('&') {
        if let Some(eq_pos) = pair.find('=') {
            let key = String::from(&pair[..eq_pos]);
            let value = String::from(&pair[eq_pos + 1..]);
            map.insert(key, value);
        } else if !pair.is_empty() {
            map.insert(String::from(pair), String::new());
        }
    }
    map
}

/// Guess Content-Type from file extension
pub fn content_type_for_path(path: &str) -> &'static str {
    if let Some(dot) = path.rfind('.') {
        match &path[dot + 1..] {
            "html" | "htm" => "text/html; charset=utf-8",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "xml" => "application/xml",
            "txt" => "text/plain",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "pdf" => "application/pdf",
            "zip" => "application/zip",
            "tar" => "application/x-tar",
            "gz" => "application/gzip",
            _ => "application/octet-stream",
        }
    } else {
        "application/octet-stream"
    }
}

/// Simple route handler
pub type RouteHandler = fn(&HttpRequest) -> HttpResponse;

/// HTTP server route
struct Route {
    method: HttpMethod,
    path: String,
    handler: RouteHandler,
}

/// Simple HTTP server
pub struct HttpServer {
    routes: Vec<Route>,
    port: u16,
    name: String,
}

impl HttpServer {
    pub fn new(port: u16) -> Self {
        Self {
            routes: Vec::new(),
            port,
            name: String::from("KnoxOS HTTP Server"),
        }
    }

    /// Register a route handler
    pub fn route(&mut self, method: HttpMethod, path: &str, handler: RouteHandler) {
        self.routes.push(Route {
            method,
            path: String::from(path),
            handler,
        });
    }

    /// Handle an incoming request
    pub fn handle_request(&self, request: &HttpRequest) -> HttpResponse {
        // Find matching route
        for route in &self.routes {
            if route.method == request.method && route.path == request.path {
                return (route.handler)(request);
            }
        }

        // Static file serving from VFS
        if request.method == HttpMethod::Get {
            let file_path = if request.path == "/" {
                String::from("/var/www/index.html")
            } else {
                format!("/var/www{}", request.path)
            };

            let vfs = crate::vfs::VFS.lock();
            if let Some(data) = vfs.read_file(&file_path) {
                let content_type = content_type_for_path(&request.path);
                return HttpResponse::new(StatusCode::Ok).with_body(data, content_type);
            }
        }

        HttpResponse::not_found()
    }

    /// Process raw bytes as an HTTP request and return response bytes
    pub fn process_raw(&self, data: &[u8]) -> Vec<u8> {
        match HttpRequest::parse(data) {
            Some(request) => {
                serial_println!(
                    "[http] {} {} from client",
                    request.method.as_str(),
                    request.path
                );
                let response = self.handle_request(&request);
                response.to_bytes()
            }
            None => HttpResponse::new(StatusCode::BadRequest)
                .with_body(b"Bad Request", "text/plain")
                .to_bytes(),
        }
    }
}

lazy_static::lazy_static! {
    /// Global HTTP server instance
    static ref HTTP_SERVER: Mutex<Option<HttpServer>> = Mutex::new(None);
}

/// Default index page handler
fn default_index(_req: &HttpRequest) -> HttpResponse {
    HttpResponse::html(
        "<!DOCTYPE html>\
        <html><head><title>KnoxOS</title>\
        <style>body{font-family:sans-serif;margin:40px;background:#1a1a2e;color:#e0e0e0;}\
        h1{color:#00d4ff;}a{color:#00d4ff;}</style></head>\
        <body><h1>Welcome to KnoxOS</h1>\
        <p>KnoxOS v0.6.0 - AI-Native Operating System</p>\
        <ul>\
        <li><a href=\"/api/info\">System Info</a></li>\
        <li><a href=\"/api/processes\">Process List</a></li>\
        <li><a href=\"/api/uptime\">Uptime</a></li>\
        </ul></body></html>",
    )
}

/// System info API handler
fn api_info(_req: &HttpRequest) -> HttpResponse {
    let procs = crate::process::PROCESS_TABLE.lock().count();
    let ticks = crate::interrupts::get_ticks();
    let uptime = ticks as f64 / 18.2;

    HttpResponse::json(&format!(
        "{{\"os\":\"KnoxOS\",\"version\":\"0.6.0\",\"arch\":\"x86_64\",\
        \"processes\":{},\"uptime_secs\":{:.1},\"kernel\":\"knoxos-kernel\"}}",
        procs, uptime
    ))
}

/// Process list API handler
fn api_processes(_req: &HttpRequest) -> HttpResponse {
    let table = crate::process::PROCESS_TABLE.lock();
    let mut json = String::from("[");
    for (i, proc) in table.list_processes().iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"pid\":{},\"name\":\"{}\",\"state\":\"{:?}\",\"uid\":{}}}",
            proc.pid, proc.name, proc.state, proc.uid
        ));
    }
    json.push(']');
    HttpResponse::json(&json)
}

/// Uptime API handler
fn api_uptime(_req: &HttpRequest) -> HttpResponse {
    let ticks = crate::interrupts::get_ticks();
    let secs = ticks as f64 / 18.2;
    let hours = secs as u64 / 3600;
    let mins = (secs as u64 % 3600) / 60;
    let s = secs as u64 % 60;
    HttpResponse::text(&format!("up {}h {}m {}s", hours, mins, s))
}

/// Initialize HTTP server with default routes
pub fn init_server(port: u16) {
    let mut server = HttpServer::new(port);
    server.route(HttpMethod::Get, "/", default_index);
    server.route(HttpMethod::Get, "/api/info", api_info);
    server.route(HttpMethod::Get, "/api/processes", api_processes);
    server.route(HttpMethod::Get, "/api/uptime", api_uptime);

    *HTTP_SERVER.lock() = Some(server);
    serial_println!("[KnoxOS] HTTP server initialized on port {}", port);
}

/// Handle incoming HTTP data
pub fn handle_incoming(data: &[u8]) -> Vec<u8> {
    let server = HTTP_SERVER.lock();
    match server.as_ref() {
        Some(s) => s.process_raw(data),
        None => HttpResponse::new(StatusCode::ServiceUnavailable)
            .with_body(b"Server not started", "text/plain")
            .to_bytes(),
    }
}

/// Build an HTTP GET request
pub fn build_get_request(host: &str, path: &str) -> Vec<u8> {
    let mut req = HttpRequest {
        method: HttpMethod::Get,
        path: String::from(path),
        version: HttpVersion::Http11,
        headers: HttpHeaders::new(),
        body: Vec::new(),
        query: BTreeMap::new(),
    };
    req.headers.set("Host", host);
    req.headers.set("User-Agent", "KnoxOS/0.6.0");
    req.headers.set("Accept", "*/*");
    req.headers.set("Connection", "close");
    req.to_bytes()
}

/// Build an HTTP POST request
pub fn build_post_request(host: &str, path: &str, body: &[u8], content_type: &str) -> Vec<u8> {
    let mut req = HttpRequest {
        method: HttpMethod::Post,
        path: String::from(path),
        version: HttpVersion::Http11,
        headers: HttpHeaders::new(),
        body: body.to_vec(),
        query: BTreeMap::new(),
    };
    req.headers.set("Host", host);
    req.headers.set("User-Agent", "KnoxOS/0.6.0");
    req.headers.set("Content-Type", content_type);
    req.headers
        .set("Content-Length", &format!("{}", body.len()));
    req.headers.set("Connection", "close");
    req.to_bytes()
}

/// Perform a simple HTTP GET request and return the response body
pub fn get(url: &str) -> Result<Vec<u8>, ()> {
    serial_println!("[HTTP] GET {}", url);

    // Use the full HTTP client engine from the shell networking subsystem,
    // which handles DNS resolution, socket creation, and response generation.
    use crate::shell::builtins::net::{http_get, parse_url};

    let parsed = parse_url(url).map_err(|e| {
        serial_println!("[HTTP] GET {} -> URL parse error: {}", url, e);
    })?;

    let result = http_get(&parsed, &[], true, 20).map_err(|e| {
        serial_println!("[HTTP] GET {} -> request error: {}", url, e);
    })?;

    if result.status_code >= 400 {
        serial_println!(
            "[HTTP] GET {} -> {} {}",
            url,
            result.status_code,
            result.status_text
        );
        return Err(());
    }

    serial_println!(
        "[HTTP] GET {} -> {} {} ({} bytes)",
        url,
        result.status_code,
        result.status_text,
        result.body.len()
    );
    Ok(result.body)
}

/// Initialize HTTP subsystem
pub fn init() {
    // Create /var/www directory for static files
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.mkdir("/var/www", 0o755);
    drop(vfs);

    // Initialize server on port 8080
    init_server(8080);
}
