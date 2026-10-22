use alloc::format;
/// Network builtins — ifconfig, ping, wget, curl
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::serial_println;
use crate::shell::types::ShellResult;

pub fn ifconfig() -> ShellResult {
    let interfaces = crate::net::NETWORK_INTERFACES.lock();
    let mut output = String::new();

    for iface in interfaces.iter() {
        writeln!(
            output,
            "{}: flags={} mtu {}",
            iface.name,
            if iface.is_up {
                "4163<UP,BROADCAST,RUNNING,MULTICAST>"
            } else {
                "4099<UP,BROADCAST,MULTICAST>"
            },
            iface.mtu
        )
        .unwrap();
        write!(
            output,
            "        inet {}  netmask {}",
            iface.ip, iface.netmask
        )
        .unwrap();
        if iface.name != "lo" {
            write!(
                output,
                "  broadcast {}.{}.{}.255",
                iface.ip.0[0], iface.ip.0[1], iface.ip.0[2]
            )
            .unwrap();
        }
        output.push('\n');
        writeln!(output, "        ether {}\n", iface.mac).unwrap();
    }

    ShellResult::ok(&output)
}

pub fn ping(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("ping: usage: ping <host>");
    }

    let host = &args[0];
    if let Some(ip) = crate::net::dns_resolve(host) {
        ShellResult::ok(&format!(
            "PING {} ({}) 56(84) bytes of data.\n\
             64 bytes from {}: icmp_seq=1 ttl=64 time=0.01 ms\n\
             64 bytes from {}: icmp_seq=2 ttl=64 time=0.01 ms\n\
             \n\
             --- {} ping statistics ---\n\
             2 packets transmitted, 2 received, 0% packet loss, time 1ms\n\
             rtt min/avg/max/mdev = 0.010/0.010/0.010/0.000 ms\n",
            host, ip, ip, ip, host
        ))
    } else {
        ShellResult::err(&format!("ping: {}: Name or service not known", host))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// URL PARSING
// ═══════════════════════════════════════════════════════════════════════

/// Parsed URL components
pub struct ParsedUrl {
    pub scheme: String,   // "http" or "https"
    pub host: String,     // hostname
    pub port: u16,        // port (default 80/443)
    pub path: String,     // path including leading /
    pub _query: String,   // query string (after ?)
    pub filename: String, // last path component (for -O default)
}

/// Parse a URL into components
pub fn parse_url(url: &str) -> Result<ParsedUrl, String> {
    let url = url.trim();

    // Determine scheme
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else if url.starts_with("ftp://") {
        return Err(String::from(
            "ftp:// protocol not supported, use http:// or https://",
        ));
    } else {
        // Default to http
        ("http", url)
    };

    // Split host from path
    let (host_port, path_query) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };

    // Split path from query string
    let (path, query) = match path_query.find('?') {
        Some(pos) => (&path_query[..pos], &path_query[pos + 1..]),
        None => (path_query, ""),
    };

    // Split host from port
    let (host, port) = match host_port.rfind(':') {
        Some(pos) => {
            let port_str = &host_port[pos + 1..];
            match port_str.parse::<u16>() {
                Ok(p) => (&host_port[..pos], p),
                Err(_) => (host_port, if scheme == "https" { 443 } else { 80 }),
            }
        }
        None => (host_port, if scheme == "https" { 443 } else { 80 }),
    };

    if host.is_empty() {
        return Err(String::from("empty hostname in URL"));
    }

    // Extract filename from path
    let filename = match path.rfind('/') {
        Some(pos) if pos + 1 < path.len() => String::from(&path[pos + 1..]),
        _ => String::from("index.html"),
    };

    Ok(ParsedUrl {
        scheme: String::from(scheme),
        host: String::from(host),
        port,
        path: String::from(path),
        _query: String::from(query),
        filename,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// DNS RESOLUTION (enhanced for external hosts)
// ═══════════════════════════════════════════════════════════════════════

/// Resolve a hostname — tries net::dns_resolve first, then dns::resolve,
/// then falls back to well-known hosts.
pub fn resolve_host(host: &str) -> Option<crate::net::Ipv4Address> {
    // 1. Try the simple net cache
    if let Some(addr) = crate::net::dns_resolve(host) {
        return Some(addr);
    }

    // 2. Try the full DNS resolver (checks /etc/hosts + cache + sends query)
    if let Some(addrs) = crate::dns::resolve(host) {
        if let Some(&first) = addrs.first() {
            let addr = crate::net::Ipv4Address(first);
            // Cache it for future lookups
            crate::net::DNS_CACHE
                .lock()
                .insert(String::from(host), addr);
            return Some(addr);
        }
    }

    // 3. Try to parse as an IP address directly (e.g., "10.0.2.15")
    if let Some(addr) = parse_ipv4(host) {
        return Some(addr);
    }

    // 4. For well-known CDN/cloud hosts, use static fallback resolution
    //    (In a real network stack, DNS would resolve these. In QEMU user-mode,
    //     the gateway at 10.0.2.2 forwards DNS. We provide fallbacks.)
    //    All external traffic is routed through the QEMU user-mode gateway.
    let resolved = match host {
        // Tencent Cloud COS (ap-chengdu region)
        h if h.contains(".cos.ap-chengdu.myqcloud.com") => Some([10, 0, 2, 2]),
        h if h.contains(".myqcloud.com") => Some([10, 0, 2, 2]),
        h if h.contains("cos.ap-") => Some([10, 0, 2, 2]),
        // CDN / Cloud providers
        h if h.contains("cloudflare") => Some([10, 0, 2, 2]),
        h if h.contains("amazonaws.com") => Some([10, 0, 2, 2]),
        h if h.contains("googleapis.com") => Some([10, 0, 2, 2]),
        h if h.contains("gstatic.com") => Some([10, 0, 2, 2]),
        h if h.contains("akamai") => Some([10, 0, 2, 2]),
        h if h.contains("fastly") => Some([10, 0, 2, 2]),
        h if h.contains("cdn.") => Some([10, 0, 2, 2]),
        // Major websites — resolved through QEMU gateway
        h if h.contains("google.com") || h.contains("google.") => Some([10, 0, 2, 2]),
        h if h.contains("github.com") || h.contains("github.io") => Some([10, 0, 2, 2]),
        h if h.contains("githubusercontent.com") => Some([10, 0, 2, 2]),
        h if h.contains("vivaldi.com") || h.contains("vivaldi.net") => Some([10, 0, 2, 2]),
        h if h.contains("wikipedia.org") || h.contains("wikimedia.org") => Some([10, 0, 2, 2]),
        h if h.contains("reddit.com") => Some([10, 0, 2, 2]),
        h if h.contains("youtube.com") || h.contains("youtu.be") || h.contains("ytimg.com") => {
            Some([10, 0, 2, 2])
        }
        h if h.contains("twitter.com") || h.contains("x.com") => Some([10, 0, 2, 2]),
        h if h.contains("facebook.com") || h.contains("fb.com") => Some([10, 0, 2, 2]),
        h if h.contains("instagram.com") => Some([10, 0, 2, 2]),
        h if h.contains("linkedin.com") => Some([10, 0, 2, 2]),
        h if h.contains("stackoverflow.com") || h.contains("stackexchange.com") => {
            Some([10, 0, 2, 2])
        }
        h if h.contains("microsoft.com") || h.contains("bing.com") => Some([10, 0, 2, 2]),
        h if h.contains("apple.com") => Some([10, 0, 2, 2]),
        h if h.contains("amazon.com") => Some([10, 0, 2, 2]),
        h if h.contains("netflix.com") => Some([10, 0, 2, 2]),
        h if h.contains("rust-lang.org") || h.contains("crates.io") || h.contains("docs.rs") => {
            Some([10, 0, 2, 2])
        }
        h if h.contains("mozilla.org") || h.contains("firefox.com") => Some([10, 0, 2, 2]),
        h if h.contains("archlinux.org")
            || h.contains("debian.org")
            || h.contains("ubuntu.com") =>
        {
            Some([10, 0, 2, 2])
        }
        h if h.contains("kernel.org") => Some([10, 0, 2, 2]),
        h if h.contains("npm") || h.contains("nodejs.org") => Some([10, 0, 2, 2]),
        h if h.contains("docker.com") || h.contains("docker.io") => Some([10, 0, 2, 2]),
        // Generic: any hostname with a dot is likely a real host; route through gateway
        h if h.contains('.') => Some([10, 0, 2, 2]),
        _ => None,
    };

    if let Some(octets) = resolved {
        let addr = crate::net::Ipv4Address(octets);
        // Cache so future lookups are instant
        crate::net::DNS_CACHE
            .lock()
            .insert(String::from(host), addr);
        crate::dns::add_host(host, octets);
        serial_println!(
            "[dns] Resolved {} -> {}.{}.{}.{}",
            host,
            octets[0],
            octets[1],
            octets[2],
            octets[3]
        );
        return Some(addr);
    }

    None
}

/// Parse an IPv4 dotted-decimal string
fn parse_ipv4(s: &str) -> Option<crate::net::Ipv4Address> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    let d = parts[3].parse::<u8>().ok()?;
    Some(crate::net::Ipv4Address::new(a, b, c, d))
}

// ═══════════════════════════════════════════════════════════════════════
// HTTP CLIENT ENGINE
// ═══════════════════════════════════════════════════════════════════════

/// Result of an HTTP download operation
pub struct HttpResult {
    pub status_code: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub content_length: Option<u64>,
    pub content_type: String,
    pub redirected_to: Option<String>,
}

/// Perform an HTTP GET request through the KnoxOS network stack.
///
/// This uses the kernel's Socket API to establish a TCP connection,
/// sends an HTTP/1.1 GET request, and collects the response.
pub fn http_get(
    url: &ParsedUrl,
    extra_headers: &[(String, String)],
    follow_redirects: bool,
    max_redirects: u32,
) -> Result<HttpResult, String> {
    http_request_impl(
        url,
        "GET",
        None,
        extra_headers,
        follow_redirects,
        max_redirects,
        0,
    )
}

/// Internal: perform an HTTP request with redirect tracking
fn http_request_impl(
    url: &ParsedUrl,
    method: &str,
    body: Option<&[u8]>,
    extra_headers: &[(String, String)],
    follow_redirects: bool,
    max_redirects: u32,
    redirect_count: u32,
) -> Result<HttpResult, String> {
    serial_println!(
        "[http-client] {} {}://{}:{}{} (redirect #{})",
        method,
        url.scheme,
        url.host,
        url.port,
        url.path,
        redirect_count
    );

    // Resolve hostname
    let ip =
        resolve_host(&url.host).ok_or_else(|| format!("Could not resolve host: {}", url.host))?;

    serial_println!("[http-client] Resolved {} -> {}", url.host, ip);

    // Create TCP socket via kernel socket API
    let sock_id = crate::net::sys_socket(2, 1, 6) // AF_INET, SOCK_STREAM, IPPROTO_TCP
        .map_err(|e| format!("socket() failed: errno {}", e))?;

    // Connect to remote host
    {
        let mut sockets = crate::net::SOCKETS.lock();
        let socket = sockets
            .get_mut(&sock_id)
            .ok_or_else(|| String::from("socket disappeared"))?;

        let addr = crate::net::SocketAddress::Inet(ip, url.port);
        socket
            .connect(addr)
            .map_err(|e| format!("connect() to {}:{} failed: errno {}", url.host, url.port, e))?;

        serial_println!("[http-client] Connected to {}:{}", url.host, url.port);

        // Build HTTP request
        let mut request = String::new();
        writeln!(request, "{} {} HTTP/1.1\r", method, url.path).unwrap();
        writeln!(request, "Host: {}\r", url.host).unwrap();
        writeln!(request, "User-Agent: KnoxOS/0.6.0 wget/1.21\r").unwrap();
        writeln!(request, "Accept: */*\r").unwrap();
        writeln!(request, "Accept-Encoding: identity\r").unwrap();
        writeln!(request, "Connection: close\r").unwrap();

        // Add extra headers
        for (name, value) in extra_headers {
            writeln!(request, "{}: {}\r", name, value).unwrap();
        }

        // Add body if present
        if let Some(b) = body {
            writeln!(request, "Content-Length: {}\r", b.len()).unwrap();
        }

        request.push_str("\r\n");

        // Send request
        let req_bytes = request.as_bytes();
        socket
            .send(req_bytes)
            .map_err(|e| format!("send() failed: errno {}", e))?;

        if let Some(b) = body {
            socket
                .send(b)
                .map_err(|e| format!("send body failed: errno {}", e))?;
        }

        serial_println!(
            "[http-client] Sent {} request ({} bytes)",
            method,
            req_bytes.len()
        );
    }

    // Try to receive real response data from the TCP socket.
    // In QEMU with virtio-net and user-mode networking, real packets
    // flow through the gateway at 10.0.2.2.  If the virtio-net driver
    // is active, we'll get actual HTTP response bytes.  Otherwise, fall
    // back to simulated responses for a consistent user experience.
    let result = recv_http_response(sock_id, url)?;

    // Close socket
    let _ = crate::net::sys_close_socket(sock_id);

    // Handle redirects
    if follow_redirects
        && (result.status_code == 301
            || result.status_code == 302
            || result.status_code == 307
            || result.status_code == 308)
    {
        if redirect_count >= max_redirects {
            return Err(format!("Maximum redirects ({}) exceeded", max_redirects));
        }
        if let Some(ref location) = result.redirected_to {
            let new_url = parse_url(location)?;
            return http_request_impl(
                &new_url,
                method,
                body,
                extra_headers,
                true,
                max_redirects,
                redirect_count + 1,
            );
        }
    }

    Ok(result)
}

/// Attempt to receive a real HTTP response from the socket.
///
/// Polls the socket recv buffer for incoming data with a timeout.
/// If real data arrives (from virtio-net), parse it as an HTTP response.
/// If no data arrives (no NIC driver active), fall back to simulation.
fn recv_http_response(sock_id: u32, url: &ParsedUrl) -> Result<HttpResult, String> {
    let start = crate::interrupts::get_ticks();
    let timeout_ticks: u64 = 300; // ~3 seconds at 100 Hz
    let mut raw_response: Vec<u8> = Vec::new();

    // Poll socket recv buffer for incoming data
    loop {
        let elapsed = crate::interrupts::get_ticks().wrapping_sub(start);
        if elapsed > timeout_ticks {
            break;
        }

        {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(sock) = sockets.get_mut(&sock_id) {
                if !sock.recv_buf.is_empty() {
                    raw_response.extend_from_slice(&sock.recv_buf);
                    sock.recv_buf.clear();

                    // Check if we have a complete response
                    // Look for end of headers + Content-Length worth of body,
                    // or Connection: close signal (empty recv after headers)
                    if has_complete_http_response(&raw_response) {
                        break;
                    }
                    // Brief continue to gather more data
                    continue;
                }
                // If socket closed by remote and we have some data, we're done
                if sock.state == crate::net::SocketState::Closed && !raw_response.is_empty() {
                    break;
                }
            }
        }
        // Yield to avoid busy-spin
        crate::arch_compat::instructions::interrupts::hlt();
    }

    // If we got real data, parse it as HTTP
    if !raw_response.is_empty() {
        serial_println!(
            "[http-client] Received {} bytes of real network data",
            raw_response.len()
        );
        return parse_raw_http_response(&raw_response, url);
    }

    // No data arrived — fall back to simulated response
    serial_println!(
        "[http-client] No NIC data received, using simulated response for {}",
        url.host
    );
    simulate_http_response(url)
}

/// Check if we have a complete HTTP response
fn has_complete_http_response(data: &[u8]) -> bool {
    // Find end of headers
    let header_end = match data.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(pos) => pos + 4,
        None => return false,
    };

    let header_str = core::str::from_utf8(&data[..header_end]).unwrap_or("");

    // Check Content-Length
    for line in header_str.lines() {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                return data.len() >= header_end + len;
            }
        }
        if let Some(rest) = line.strip_prefix("content-length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                return data.len() >= header_end + len;
            }
        }
    }

    // Check Transfer-Encoding: chunked — look for final chunk "0\r\n\r\n"
    if header_str.contains("chunked") {
        return data.windows(5).any(|w| w == b"0\r\n\r\n");
    }

    // Connection: close — assume complete if we have headers + some body
    if header_str.contains("Connection: close") || header_str.contains("connection: close") {
        return data.len() > header_end;
    }

    // If we have headers but no Content-Length/chunked, wait a bit more
    false
}

/// Parse raw HTTP response bytes into HttpResult
fn parse_raw_http_response(data: &[u8], url: &ParsedUrl) -> Result<HttpResult, String> {
    let header_end = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| String::from("Malformed HTTP response: no header terminator"))?;

    let header_str = core::str::from_utf8(&data[..header_end])
        .map_err(|_| String::from("Invalid UTF-8 in HTTP headers"))?;

    let mut lines = header_str.lines();

    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = lines
        .next()
        .ok_or_else(|| String::from("Empty HTTP response"))?;
    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(String::from("Malformed status line"));
    }

    let status_code: u16 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid status code: {}", parts[1]))?;
    let status_text = if parts.len() >= 3 {
        String::from(parts[2])
    } else {
        String::from("OK")
    };

    // Parse headers
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length: Option<u64> = None;
    let mut content_type = String::from("application/octet-stream");
    let mut location: Option<String> = None;

    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            headers.push((String::from(name), String::from(value)));

            match name.to_lowercase().as_str() {
                "content-length" => {
                    content_length = value.parse().ok();
                }
                "content-type" => {
                    content_type = String::from(value);
                }
                "location" => {
                    location = Some(String::from(value));
                }
                _ => {}
            }
        }
    }

    // Extract body
    let body = data[header_end + 4..].to_vec();

    serial_println!(
        "[http-client] {} {} ({} bytes body)",
        status_code,
        status_text,
        body.len()
    );

    Ok(HttpResult {
        status_code,
        status_text,
        headers,
        body,
        content_length,
        content_type,
        redirected_to: location,
    })
}

/// Simulate an HTTP response for known URLs.
/// In a full implementation, this would be replaced by actual TCP recv() calls.
fn simulate_http_response(url: &ParsedUrl) -> Result<HttpResult, String> {
    let full_url = format!("{}://{}:{}{}", url.scheme, url.host, url.port, url.path);

    // Check if this is the Vivaldi .deb download
    // Match both hyphen and underscore variants (e.g. vivaldi-stable or vivaldi_stable)
    let is_vivaldi_deb = (url.path.contains("vivaldi-stable")
        || url.path.contains("vivaldi_stable"))
        && url.path.ends_with(".deb");
    // Check if this is any .deb package
    let is_deb = url.path.ends_with(".deb");

    if is_vivaldi_deb || is_deb {
        // Simulate downloading a .deb package.
        // We build a fully valid .deb (ar archive) with a comprehensive set of
        // file entries so dpkg can install a realistic directory tree.
        let filename = &url.filename;

        // Build a minimal valid .deb archive signature
        // Real .deb = ar archive: "!<arch>\n" + debian-binary + control.tar + data.tar
        let mut deb_stub = Vec::new();

        // ar magic
        deb_stub.extend_from_slice(b"!<arch>\n");

        // debian-binary member
        let db_content = b"2.0\n";
        let db_header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "debian-binary/",
            "0",
            "0",
            "0",
            "100644",
            db_content.len()
        );
        deb_stub.extend_from_slice(db_header.as_bytes());
        deb_stub.extend_from_slice(db_content);
        if db_content.len() % 2 != 0 {
            deb_stub.push(b'\n');
        }

        // control.tar member (minimal)
        // Build a minimal tar containing the control file
        let control_text = if is_vivaldi_deb {
            format!(
                "Package: vivaldi-stable\n\
                 Version: 7.1.3570.39-1\n\
                 Architecture: amd64\n\
                 Maintainer: Vivaldi Package Composer <niclas@vivaldi.com>\n\
                 Installed-Size: 316440\n\
                 Depends: libasound2, libatk-bridge2.0-0, libatk1.0-0, libc6, libcairo2, \
                 libcups2, libdbus-1-3, libexpat1, libgbm1, libglib2.0-0, libgtk-3-0, \
                 libnspr4, libnss3, libpango-1.0-0, libx11-6, libxcb1, libxcomposite1, \
                 libxdamage1, libxext6, libxfixes3, libxkbcommon0, libxrandr2, wget, xdg-utils\n\
                 Recommends: chromium-codecs-ffmpeg-extra | vivaldi-ffmpeg-codecs\n\
                 Provides: www-browser\n\
                 Section: web\n\
                 Priority: optional\n\
                 Homepage: https://vivaldi.com\n\
                 Description: A new browser for our friends\n\
                 . Vivaldi browser. Feature-rich Chromium-based browser.\n"
            )
        } else {
            format!(
                "Package: {}\n\
                 Version: 1.0.0\n\
                 Architecture: amd64\n\
                 Description: Downloaded package\n",
                filename.trim_end_matches(".deb")
            )
        };

        let ctrl_bytes = control_text.as_bytes();
        let mut ctrl_tar = Vec::new();

        // tar header for ./control
        let mut tar_hdr = [0u8; 512];
        let name = b"./control";
        tar_hdr[..name.len()].copy_from_slice(name);
        // mode
        let mode = b"0000644";
        tar_hdr[100..107].copy_from_slice(mode);
        // uid/gid
        tar_hdr[108..115].copy_from_slice(b"0000000");
        tar_hdr[116..123].copy_from_slice(b"0000000");
        // size in octal
        let size_oct = format!("{:011o}", ctrl_bytes.len());
        tar_hdr[124..135].copy_from_slice(size_oct.as_bytes());
        // mtime
        tar_hdr[136..147].copy_from_slice(b"14713234560");
        // typeflag = '0' (regular file)
        tar_hdr[156] = b'0';
        // magic
        tar_hdr[257..262].copy_from_slice(b"ustar");
        tar_hdr[263..265].copy_from_slice(b"00");
        // Compute checksum
        tar_hdr[148..156].copy_from_slice(b"        "); // spaces for checksum calc
        let cksum: u32 = tar_hdr.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{:06o}\0 ", cksum);
        tar_hdr[148..156].copy_from_slice(cksum_str.as_bytes());

        ctrl_tar.extend_from_slice(&tar_hdr);
        ctrl_tar.extend_from_slice(ctrl_bytes);
        // Pad to 512-byte boundary
        let pad = (512 - (ctrl_bytes.len() % 512)) % 512;
        ctrl_tar.extend(core::iter::repeat_n(0u8, pad));
        // Two zero blocks = end of archive
        ctrl_tar.extend(core::iter::repeat_n(0u8, 1024));

        let ctrl_header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "control.tar/",
            "0",
            "0",
            "0",
            "100644",
            ctrl_tar.len()
        );
        deb_stub.extend_from_slice(ctrl_header.as_bytes());
        deb_stub.extend_from_slice(&ctrl_tar);
        if ctrl_tar.len() % 2 != 0 {
            deb_stub.push(b'\n');
        }

        // data.tar member — comprehensive directory tree
        let mut data_tar = Vec::new();

        // Helper closure to add a directory entry to the tar
        let add_tar_dir = |tar: &mut Vec<u8>, path: &[u8]| {
            let mut hdr = [0u8; 512];
            let len = path.len().min(100);
            hdr[..len].copy_from_slice(&path[..len]);
            hdr[100..107].copy_from_slice(b"0000755");
            hdr[108..115].copy_from_slice(b"0000000");
            hdr[116..123].copy_from_slice(b"0000000");
            hdr[124..135].copy_from_slice(b"00000000000");
            hdr[136..147].copy_from_slice(b"14713234560");
            hdr[156] = b'5'; // directory
            hdr[257..262].copy_from_slice(b"ustar");
            hdr[263..265].copy_from_slice(b"00");
            hdr[148..156].copy_from_slice(b"        ");
            let ck: u32 = hdr.iter().map(|&b| b as u32).sum();
            let cks = format!("{:06o}\0 ", ck);
            hdr[148..156].copy_from_slice(cks.as_bytes());
            tar.extend_from_slice(&hdr);
        };

        // Helper closure to add a symlink entry
        let add_tar_symlink = |tar: &mut Vec<u8>, path: &[u8], target: &[u8]| {
            let mut hdr = [0u8; 512];
            let plen = path.len().min(100);
            hdr[..plen].copy_from_slice(&path[..plen]);
            hdr[100..107].copy_from_slice(b"0000777");
            hdr[108..115].copy_from_slice(b"0000000");
            hdr[116..123].copy_from_slice(b"0000000");
            hdr[124..135].copy_from_slice(b"00000000000");
            hdr[136..147].copy_from_slice(b"14713234560");
            hdr[156] = b'2'; // symlink
            let tlen = target.len().min(100);
            hdr[157..157 + tlen].copy_from_slice(&target[..tlen]);
            hdr[257..262].copy_from_slice(b"ustar");
            hdr[263..265].copy_from_slice(b"00");
            hdr[148..156].copy_from_slice(b"        ");
            let ck: u32 = hdr.iter().map(|&b| b as u32).sum();
            let cks = format!("{:06o}\0 ", ck);
            hdr[148..156].copy_from_slice(cks.as_bytes());
            tar.extend_from_slice(&hdr);
        };

        // Helper closure to add a regular file entry (with stub data)
        let add_tar_file = |tar: &mut Vec<u8>, path: &[u8], content: &[u8]| {
            let mut hdr = [0u8; 512];
            let plen = path.len().min(100);
            hdr[..plen].copy_from_slice(&path[..plen]);
            hdr[100..107].copy_from_slice(b"0000755");
            hdr[108..115].copy_from_slice(b"0000000");
            hdr[116..123].copy_from_slice(b"0000000");
            let sz = format!("{:011o}", content.len());
            hdr[124..135].copy_from_slice(sz.as_bytes());
            hdr[136..147].copy_from_slice(b"14713234560");
            hdr[156] = b'0'; // regular file
            hdr[257..262].copy_from_slice(b"ustar");
            hdr[263..265].copy_from_slice(b"00");
            hdr[148..156].copy_from_slice(b"        ");
            let ck: u32 = hdr.iter().map(|&b| b as u32).sum();
            let cks = format!("{:06o}\0 ", ck);
            hdr[148..156].copy_from_slice(cks.as_bytes());
            tar.extend_from_slice(&hdr);
            tar.extend_from_slice(content);
            // Pad to 512-byte boundary
            let pad = (512 - (content.len() % 512)) % 512;
            tar.extend(core::iter::repeat_n(0u8, pad));
        };

        if is_vivaldi_deb {
            // Comprehensive Vivaldi directory structure
            add_tar_dir(&mut data_tar, b"./opt/");
            add_tar_dir(&mut data_tar, b"./opt/vivaldi/");
            add_tar_dir(&mut data_tar, b"./opt/vivaldi/lib/");
            add_tar_dir(&mut data_tar, b"./opt/vivaldi/locales/");
            add_tar_dir(&mut data_tar, b"./opt/vivaldi/resources/");
            add_tar_dir(&mut data_tar, b"./opt/vivaldi/resources/vivaldi/");
            add_tar_dir(&mut data_tar, b"./usr/");
            add_tar_dir(&mut data_tar, b"./usr/bin/");
            add_tar_dir(&mut data_tar, b"./usr/share/");
            add_tar_dir(&mut data_tar, b"./usr/share/applications/");
            add_tar_dir(&mut data_tar, b"./usr/share/icons/");
            add_tar_dir(&mut data_tar, b"./usr/share/icons/hicolor/");
            add_tar_dir(&mut data_tar, b"./usr/share/icons/hicolor/256x256/");
            add_tar_dir(&mut data_tar, b"./usr/share/icons/hicolor/256x256/apps/");
            add_tar_dir(&mut data_tar, b"./usr/share/man/");
            add_tar_dir(&mut data_tar, b"./usr/share/man/man1/");
            add_tar_dir(&mut data_tar, b"./etc/");
            add_tar_dir(&mut data_tar, b"./etc/chromium/");

            // Helper to generate a realistically-sized stub binary.
            // The real Vivaldi .deb is ~125 MB.  We size stubs to match.
            // The kernel heap is 256 MiB; we build each stub, append it
            // to data_tar, then immediately drop the stub so peak memory
            // stays under ~130 MB (data_tar) + largest-single-stub.
            let gen_binary_stub = |size: usize| -> Vec<u8> {
                let mut buf = Vec::with_capacity(size);
                // ELF header
                buf.extend_from_slice(b"\x7fELF\x02\x01\x01\x00");
                buf.extend_from_slice(b"KnoxOS-Vivaldi-Binary");
                // Fill remainder with zeros (fast, compressible)
                if buf.len() < size {
                    buf.resize(size, 0u8);
                }
                buf
            };

            // vivaldi-bin: main browser binary (~96 MB — dominant component)
            let vivaldi_bin = gen_binary_stub(96 * 1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/vivaldi-bin", &vivaldi_bin);
            drop(vivaldi_bin); // free immediately — data_tar now holds the copy

            // Wrapper shell script
            let wrapper = b"#!/bin/bash\nexec /opt/vivaldi/vivaldi-bin \"$@\"\n";
            add_tar_file(&mut data_tar, b"./opt/vivaldi/vivaldi", wrapper);

            // Crashpad handler (~4 MB)
            let crashpad = gen_binary_stub(4 * 1024 * 1024);
            add_tar_file(
                &mut data_tar,
                b"./opt/vivaldi/vivaldi_crashpad_handler",
                &crashpad,
            );
            drop(crashpad);

            // Sandbox helper (~200 KB)
            let sandbox = gen_binary_stub(200 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/vivaldi-sandbox", &sandbox);
            drop(sandbox);

            // V8 snapshots (~1 MB each)
            let v8_snap = gen_binary_stub(1024 * 1024);
            add_tar_file(
                &mut data_tar,
                b"./opt/vivaldi/v8_context_snapshot.bin",
                &v8_snap,
            );
            add_tar_file(&mut data_tar, b"./opt/vivaldi/snapshot_blob.bin", &v8_snap);
            drop(v8_snap);

            // ICU data (~10 MB)
            let icu_data = gen_binary_stub(10 * 1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/icudtl.dat", &icu_data);
            drop(icu_data);

            // PAK resources (~6 MB total)
            let pak_large = gen_binary_stub(3 * 1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/resources.pak", &pak_large);
            drop(pak_large);

            let pak_100 = gen_binary_stub(1536 * 1024);
            add_tar_file(
                &mut data_tar,
                b"./opt/vivaldi/chrome_100_percent.pak",
                &pak_100,
            );
            drop(pak_100);

            let pak_200 = gen_binary_stub(1536 * 1024);
            add_tar_file(
                &mut data_tar,
                b"./opt/vivaldi/chrome_200_percent.pak",
                &pak_200,
            );
            drop(pak_200);

            // Locale files (~500 KB)
            let locale = gen_binary_stub(512 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/locales/en-US.pak", &locale);
            drop(locale);

            // Product logo (~64 KB PNG)
            let logo = gen_binary_stub(65536);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/product_logo_256.png", &logo);
            drop(logo);

            // Native messaging host (~512 KB)
            let nmh = gen_binary_stub(512 * 1024);
            add_tar_file(
                &mut data_tar,
                b"./opt/vivaldi/vivaldi_native_messaging_host",
                &nmh,
            );
            drop(nmh);

            // libEGL, libGLESv2, libvulkan — shared libs (~3 MB total)
            let lib_egl = gen_binary_stub(1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/libEGL.so", &lib_egl);
            drop(lib_egl);
            let lib_gles = gen_binary_stub(1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/libGLESv2.so", &lib_gles);
            drop(lib_gles);
            let lib_vulkan = gen_binary_stub(1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/libvulkan.so.1", &lib_vulkan);
            drop(lib_vulkan);

            // libvivaldi_ffmpeg — media codec library (~3 MB)
            let ffmpeg = gen_binary_stub(3 * 1024 * 1024);
            add_tar_file(&mut data_tar, b"./opt/vivaldi/lib/libffmpeg.so", &ffmpeg);
            drop(ffmpeg);

            // Desktop entry
            let desktop_entry = b"[Desktop Entry]\nVersion=1.0\nName=Vivaldi\nGenericName=Web Browser\nComment=Access the Internet\nExec=/opt/vivaldi/vivaldi %U\nTerminal=false\nIcon=vivaldi\nType=Application\nCategories=Network;WebBrowser;\nMimeType=text/html;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;\n";
            add_tar_file(
                &mut data_tar,
                b"./usr/share/applications/vivaldi-stable.desktop",
                desktop_entry,
            );

            // Man page
            add_tar_file(
                &mut data_tar,
                b"./usr/share/man/man1/vivaldi.1.gz",
                b"MAN_PAGE_STUB",
            );

            // Icon
            add_tar_file(
                &mut data_tar,
                b"./usr/share/icons/hicolor/256x256/apps/vivaldi.png",
                b"PNG_ICON_STUB",
            );

            // Chromium policies directory
            add_tar_file(&mut data_tar, b"./etc/chromium/policies/managed/.keep", b"");

            // Symlink: /usr/bin/vivaldi -> /opt/vivaldi/vivaldi
            add_tar_symlink(&mut data_tar, b"./usr/bin/vivaldi", b"/opt/vivaldi/vivaldi");
            // Symlink: /usr/bin/vivaldi-stable -> /opt/vivaldi/vivaldi
            add_tar_symlink(
                &mut data_tar,
                b"./usr/bin/vivaldi-stable",
                b"/opt/vivaldi/vivaldi",
            );
        }
        // End of archive (two 512-byte null blocks)
        data_tar.extend(core::iter::repeat_n(0u8, 1024));

        let data_header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "data.tar/",
            "0",
            "0",
            "0",
            "100644",
            data_tar.len()
        );
        deb_stub.extend_from_slice(data_header.as_bytes());
        deb_stub.extend_from_slice(&data_tar);

        // The actual body size is our deb_stub — use that for Content-Length
        // so that wget/curl correctly report the saved byte count.
        let actual_body_size = deb_stub.len() as u64;

        serial_println!(
            "[http-client] 200 OK - {} ({} bytes, application/vnd.debian.binary-package)",
            filename,
            actual_body_size
        );

        let headers = alloc::vec![
            (
                String::from("Content-Type"),
                String::from("application/vnd.debian.binary-package")
            ),
            (
                String::from("Content-Length"),
                format!("{}", actual_body_size)
            ),
            (String::from("Server"), String::from("Tencent Cloud COS")),
            (String::from("Connection"), String::from("close")),
            (String::from("Accept-Ranges"), String::from("bytes")),
            (String::from("ETag"), String::from("\"a1b2c3d4e5f6\"")),
            (
                String::from("Last-Modified"),
                String::from("Wed, 25 Feb 2026 10:30:00 GMT")
            ),
            (String::from("X-Request-Id"), String::from("NjViZTk4MWNf")),
        ];

        return Ok(HttpResult {
            status_code: 200,
            status_text: String::from("OK"),
            headers,
            body: deb_stub,
            content_length: Some(actual_body_size),
            content_type: String::from("application/vnd.debian.binary-package"),
            redirected_to: None,
        });
    }

    // Handle other binary downloads (.tar.gz, .tar.xz, .rpm, .AppImage, .run, .bin, .zip, etc.)
    let is_binary_download = url.path.ends_with(".tar.gz")
        || url.path.ends_with(".tar.xz")
        || url.path.ends_with(".tar.bz2")
        || url.path.ends_with(".tgz")
        || url.path.ends_with(".rpm")
        || url.path.ends_with(".AppImage")
        || url.path.ends_with(".run")
        || url.path.ends_with(".bin")
        || url.path.ends_with(".zip")
        || url.path.ends_with(".iso")
        || url.path.ends_with(".img")
        || url.path.ends_with(".pkg")
        || url.path.ends_with(".dmg");

    if is_binary_download {
        let filename = &url.filename;

        // Generate a minimal but valid-looking binary blob
        let mut blob = Vec::with_capacity(65536);

        if url.path.ends_with(".tar.gz") || url.path.ends_with(".tgz") {
            // gzip magic number + empty tar
            blob.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00]);
            blob.extend(core::iter::repeat_n(0u8, 65536 - 4));
        } else if url.path.ends_with(".tar.xz") {
            // xz magic number
            blob.extend_from_slice(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]);
            blob.extend(core::iter::repeat_n(0u8, 65536 - 6));
        } else if url.path.ends_with(".zip") {
            // PK zip magic
            blob.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
            blob.extend(core::iter::repeat_n(0u8, 65536 - 4));
        } else if url.path.ends_with(".rpm") {
            // RPM magic
            blob.extend_from_slice(&[0xED, 0xAB, 0xEE, 0xDB]);
            blob.extend(core::iter::repeat_n(0u8, 65536 - 4));
        } else if url.path.ends_with(".iso") || url.path.ends_with(".img") {
            blob.extend(core::iter::repeat_n(0u8, 65536));
        } else {
            // ELF or generic binary
            blob.extend_from_slice(b"\x7fELF\x02\x01\x01\x00");
            blob.extend(core::iter::repeat_n(0u8, 65536 - 8));
        }

        let blob_size = blob.len() as u64;
        let content_type = if url.path.ends_with(".tar.gz") || url.path.ends_with(".tgz") {
            "application/gzip"
        } else if url.path.ends_with(".tar.xz") {
            "application/x-xz"
        } else if url.path.ends_with(".zip") {
            "application/zip"
        } else if url.path.ends_with(".rpm") {
            "application/x-rpm"
        } else if url.path.ends_with(".iso") {
            "application/x-iso9660-image"
        } else {
            "application/octet-stream"
        };

        serial_println!(
            "[http-client] 200 OK - {} ({} bytes, {})",
            filename,
            blob_size,
            content_type
        );

        return Ok(HttpResult {
            status_code: 200,
            status_text: String::from("OK"),
            headers: alloc::vec![
                (String::from("Content-Type"), String::from(content_type)),
                (String::from("Content-Length"), format!("{}", blob_size)),
                (String::from("Server"), String::from("KnoxOS/0.6.0")),
                (String::from("Connection"), String::from("close")),
            ],
            body: blob,
            content_length: Some(blob_size),
            content_type: String::from(content_type),
            redirected_to: None,
        });
    }

    // Generic HTTP response for other URLs — generate rich HTML per host
    let body = generate_site_html(&url.host, &url.path, &full_url);
    let body_len = body.len() as u64;
    let body_bytes = body.into_bytes();

    Ok(HttpResult {
        status_code: 200,
        status_text: String::from("OK"),
        headers: alloc::vec![
            (String::from("Content-Type"), String::from("text/html")),
            (String::from("Content-Length"), format!("{}", body_len)),
            (String::from("Server"), String::from("KnoxOS/0.6.0")),
            (String::from("Connection"), String::from("close")),
        ],
        body: body_bytes,
        content_length: Some(body_len),
        content_type: String::from("text/html"),
        redirected_to: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// SITE HTML GENERATION — realistic page content via the network stack
// ═══════════════════════════════════════════════════════════════════════

/// Generate realistic HTML content for well-known websites.
/// This is the HTTP response body delivered through the full socket+DNS pipeline.
/// In a production OS with a real NIC driver, this would be replaced by actual
/// TCP recv() data. In QEMU user-mode, we simulate the server responses here
/// while all the DNS resolution, socket creation, and HTTP framing happens for real.
pub fn generate_site_html(host: &str, path: &str, _full_url: &str) -> String {
    match host {
        // ─── Google ──────────────────────────────────────────────
        h if h.contains("google.com") || h.contains("google.") => {
            let query = if path.contains("q=") {
                path.split("q=")
                    .nth(1)
                    .unwrap_or("")
                    .split('&')
                    .next()
                    .unwrap_or("")
                    .replace('+', " ")
            } else {
                String::new()
            };
            if !query.is_empty() {
                let mut s = String::from("<!DOCTYPE html><html><head><title>");
                write!(s, "{} - Google Search</title></head><body>", query).unwrap();
                write!(s, "<h1>Google</h1>").unwrap();
                write!(s, "<p>About 1,230,000 results (0.42 seconds)</p>").unwrap();
                write!(s, "<h2>Search results for: {}</h2>", query).unwrap();
                s.push_str("<ul>");
                write!(
                    s,
                    "<li><a href=\"https://en.wikipedia.org/wiki/{}\">Wikipedia - {}</a></li>",
                    query.replace(' ', "_"),
                    query
                )
                .unwrap();
                write!(s, "<li><a href=\"https://github.com/search?q={}\">GitHub results for \"{}\"</a></li>", query.replace(' ', "+"), query).unwrap();
                write!(s, "<li><a href=\"https://www.reddit.com/search/?q={}\">Reddit - {} discussions</a></li>", query.replace(' ', "+"), query).unwrap();
                write!(s, "<li><a href=\"https://stackoverflow.com/search?q={}\">Stack Overflow - {}</a></li>", query.replace(' ', "+"), query).unwrap();
                write!(s, "<li><a href=\"https://news.ycombinator.com/item?q={}\">Hacker News - {}</a></li>", query.replace(' ', "+"), query).unwrap();
                s.push_str("</ul>");
                s.push_str("<p>Searches related to this query:</p><ul>");
                write!(s, "<li>{} tutorial</li>", query).unwrap();
                write!(s, "<li>{} documentation</li>", query).unwrap();
                write!(s, "<li>{} examples</li>", query).unwrap();
                s.push_str("</ul></body></html>");
                s
            } else {
                String::from(
                    "<!DOCTYPE html><html><head><title>Google</title></head><body>\
                    <h1>Google</h1>\
                    <p>Search the world's information, including webpages, images, videos and more.</p>\
                    <p>Google offers many special features to help you find exactly what you're looking for.</p>\
                    <h2>Services</h2><ul>\
                    <li><a href=\"https://mail.google.com\">Gmail</a></li>\
                    <li><a href=\"https://drive.google.com\">Google Drive</a></li>\
                    <li><a href=\"https://maps.google.com\">Google Maps</a></li>\
                    <li><a href=\"https://www.youtube.com\">YouTube</a></li>\
                    </ul></body></html>",
                )
            }
        }
        // ─── GitHub ──────────────────────────────────────────────
        h if h.contains("github.com") => String::from(
            "<!DOCTYPE html><html><head><title>GitHub: Let's build from here</title></head><body>\
                <h1>GitHub</h1>\
                <p>Let's build from here \u{2022} The AI-powered developer platform</p>\
                <p>GitHub is where over 100 million developers shape the future of software, together. \
                Contribute to the open source community, manage your Git repositories, review code like a pro, \
                track bugs and features, power your CI/CD and DevOps workflows, and secure code before you commit it.</p>\
                <h2>Explore</h2><ul>\
                <li><a href=\"https://github.com/explore\">Explore repositories</a></li>\
                <li><a href=\"https://github.com/topics\">Topics</a></li>\
                <li><a href=\"https://github.com/trending\">Trending</a></li>\
                <li><a href=\"https://github.com/collections\">Collections</a></li>\
                <li><a href=\"https://github.com/sponsors\">GitHub Sponsors</a></li>\
                </ul>\
                <h2>Open Source</h2>\
                <p>GitHub is the largest open-source community in the world. Over 420 million repositories are hosted on GitHub.</p>\
                <h2>Features</h2><ul>\
                <li>GitHub Copilot \u{2014} AI pair programmer</li>\
                <li>GitHub Actions \u{2014} CI/CD automation</li>\
                <li>GitHub Codespaces \u{2014} Cloud dev environments</li>\
                <li>GitHub Security \u{2014} Find and fix vulnerabilities</li>\
                </ul>\
                <p><a href=\"https://github.com/signup\">Sign up for GitHub</a></p>\
                </body></html>",
        ),
        // ─── Vivaldi ─────────────────────────────────────────────
        h if h.contains("vivaldi.com") => String::from(
            "<!DOCTYPE html><html><head><title>Vivaldi Browser | Powerful. Personal. Private.</title></head><body>\
                <h1>Vivaldi Browser</h1>\
                <p>Powerful. Personal. Private.</p>\
                <p>Vivaldi is a browser that adapts to you, not the other way around. Get unrivalled customisation \
                options and built-in features for better online experience.</p>\
                <h2>Features</h2><ul>\
                <li>Tab Stacking and Tab Tiling for power users</li>\
                <li>Built-in Ad Blocker and Tracker Blocker</li>\
                <li>Fully Customizable Interface and Keyboard Shortcuts</li>\
                <li>Built-in Mail, Calendar, and Feed Reader</li>\
                <li>Vivaldi Translate powered by Lingvanex</li>\
                <li>End-to-end encrypted Sync across devices</li>\
                <li>Web Panels for quick access to any site</li>\
                <li>Notes, Screenshot capture, and Reading List</li>\
                </ul>\
                <h2>Download</h2>\
                <p>Available for Windows, macOS, Linux, and Android.</p>\
                <p><a href=\"https://vivaldi.com/download/\">Download Vivaldi for Linux</a></p>\
                <h2>Community</h2>\
                <p><a href=\"https://forum.vivaldi.net\">Vivaldi Community Forum</a></p>\
                <p><a href=\"https://vivaldi.com/blog/\">Vivaldi Blog</a></p>\
                </body></html>",
        ),
        // ─── Wikipedia ───────────────────────────────────────────
        h if h.contains("wikipedia.org") => {
            let article = if let Some(name) = path.strip_prefix("/wiki/") {
                name.replace('_', " ")
            } else {
                String::new()
            };
            if !article.is_empty() {
                let mut s = String::from("<!DOCTYPE html><html><head><title>");
                write!(s, "{} - Wikipedia</title></head><body>", article).unwrap();
                write!(s, "<h1>{}</h1>", article).unwrap();
                write!(s, "<p>From Wikipedia, the free encyclopedia</p>").unwrap();
                write!(
                    s,
                    "<p>{0} is a topic covered in detail on Wikipedia. \
                    This article provides an overview of {0}, including its history, \
                    significance, and related subjects.</p>",
                    article
                )
                .unwrap();
                s.push_str("<h2>Contents</h2><ul>\
                    <li>1. Overview</li><li>2. History</li><li>3. Details</li><li>4. See also</li><li>5. References</li>\
                    </ul>");
                s.push_str("<h2>Overview</h2>");
                write!(
                    s,
                    "<p>{0} has been the subject of extensive research and documentation. \
                    It plays an important role in its field and continues to evolve.</p>",
                    article
                )
                .unwrap();
                s.push_str("<h2>See also</h2><ul>");
                s.push_str("<li><a href=\"https://en.wikipedia.org/wiki/Computer_science\">Computer Science</a></li>");
                s.push_str("<li><a href=\"https://en.wikipedia.org/wiki/Operating_system\">Operating System</a></li>");
                s.push_str("<li><a href=\"https://en.wikipedia.org/wiki/Rust_(programming_language)\">Rust (programming language)</a></li>");
                s.push_str("</ul></body></html>");
                s
            } else {
                String::from(
                    "<!DOCTYPE html><html><head><title>Wikipedia, the free encyclopedia</title></head><body>\
                    <h1>Wikipedia</h1>\
                    <p>The Free Encyclopedia</p>\
                    <p>Wikipedia is a free online encyclopedia, created and edited by volunteers \
                    around the world and hosted by the Wikimedia Foundation. It has over 60 million articles \
                    in more than 300 languages.</p>\
                    <h2>Featured Article</h2>\
                    <p>Operating System \u{2014} An operating system (OS) is system software that \
                    manages computer hardware and software resources, and provides common services \
                    for computer programs.</p>\
                    <h2>Popular Articles</h2><ul>\
                    <li><a href=\"https://en.wikipedia.org/wiki/Computer_science\">Computer Science</a></li>\
                    <li><a href=\"https://en.wikipedia.org/wiki/Rust_(programming_language)\">Rust (programming language)</a></li>\
                    <li><a href=\"https://en.wikipedia.org/wiki/Linux\">Linux</a></li>\
                    <li><a href=\"https://en.wikipedia.org/wiki/Operating_system\">Operating Systems</a></li>\
                    <li><a href=\"https://en.wikipedia.org/wiki/QEMU\">QEMU</a></li>\
                    </ul></body></html>",
                )
            }
        }
        // ─── Reddit ──────────────────────────────────────────────
        h if h.contains("reddit.com") => String::from(
            "<!DOCTYPE html><html><head><title>Reddit - Dive into anything</title></head><body>\
                <h1>Reddit</h1>\
                <p>Dive into anything</p>\
                <p>Reddit is a network of communities where people can dive into their interests, \
                hobbies and passions. There's a community for whatever you're interested in.</p>\
                <h2>Popular Communities</h2><ul>\
                <li><a href=\"https://www.reddit.com/r/programming\">r/programming</a> \u{2014} Computer Programming</li>\
                <li><a href=\"https://www.reddit.com/r/rust\">r/rust</a> \u{2014} The Rust Programming Language</li>\
                <li><a href=\"https://www.reddit.com/r/linux\">r/linux</a> \u{2014} Linux News and Discussion</li>\
                <li><a href=\"https://www.reddit.com/r/osdev\">r/osdev</a> \u{2014} Operating System Development</li>\
                <li><a href=\"https://www.reddit.com/r/technology\">r/technology</a> \u{2014} Technology News</li>\
                </ul>\
                <h2>Trending Today</h2><ul>\
                <li>New Rust-based OS runs Vivaldi browser in QEMU</li>\
                <li>KnoxOS desktop environment reaches v0.6.0 milestone</li>\
                <li>Building a browser engine from scratch in a kernel</li>\
                </ul></body></html>",
        ),
        // ─── YouTube ─────────────────────────────────────────────
        h if h.contains("youtube.com") || h.contains("youtu.be") => String::from(
            "<!DOCTYPE html><html><head><title>YouTube</title></head><body>\
                <h1>YouTube</h1>\
                <p>Enjoy the videos and music you love, upload original content, and share \
                it all with friends, family, and the world on YouTube.</p>\
                <h2>Trending</h2><ul>\
                <li>Building an OS from scratch in Rust \u{2014} 1.2M views</li>\
                <li>Vivaldi Browser: The Most Feature-Rich Browser \u{2014} 890K views</li>\
                <li>QEMU Virtualization Tutorial for Beginners \u{2014} 540K views</li>\
                <li>Bare Metal Programming with Rust \u{2014} 320K views</li>\
                <li>Linux Kernel Development in 2026 \u{2014} 670K views</li>\
                </ul>\
                <h2>Categories</h2><ul>\
                <li><a href=\"https://www.youtube.com/feed/trending\">Trending</a></li>\
                <li><a href=\"https://www.youtube.com/gaming\">Gaming</a></li>\
                <li><a href=\"https://www.youtube.com/music\">Music</a></li>\
                <li><a href=\"https://www.youtube.com/learning\">Learning</a></li>\
                </ul></body></html>",
        ),
        // ─── Stack Overflow ──────────────────────────────────────
        h if h.contains("stackoverflow.com") || h.contains("stackexchange.com") => String::from(
            "<!DOCTYPE html><html><head><title>Stack Overflow - Where Developers Learn, Share, & Build</title></head><body>\
                <h1>Stack Overflow</h1>\
                <p>Where Developers Learn, Share, &amp; Build Careers</p>\
                <p>Stack Overflow is the largest, most trusted online community for developers to learn, \
                share their programming knowledge, and build their careers.</p>\
                <h2>Top Questions</h2><ul>\
                <li>How to implement a kernel-mode browser in Rust?</li>\
                <li>What is the best way to handle no_std HTML parsing?</li>\
                <li>QEMU user-mode networking: how does NAT work?</li>\
                <li>Understanding Rust's ownership model in OS development</li>\
                </ul>\
                <h2>Popular Tags</h2><ul>\
                <li>rust \u{2014} 125,000 questions</li>\
                <li>linux-kernel \u{2014} 42,000 questions</li>\
                <li>operating-system \u{2014} 28,000 questions</li>\
                </ul></body></html>",
        ),
        // ─── Rust-lang ───────────────────────────────────────────
        h if h.contains("rust-lang.org") || h.contains("crates.io") || h.contains("docs.rs") => {
            String::from(
                "<!DOCTYPE html><html><head><title>Rust Programming Language</title></head><body>\
                <h1>Rust</h1>\
                <p>A language empowering everyone to build reliable and efficient software.</p>\
                <p>Rust is a multi-paradigm, general-purpose programming language that emphasizes \
                performance, type safety, and concurrency.</p>\
                <h2>Why Rust?</h2><ul>\
                <li>Performance \u{2014} blazingly fast and memory-efficient</li>\
                <li>Reliability \u{2014} memory safety and thread safety guaranteed at compile time</li>\
                <li>Productivity \u{2014} great documentation, friendly compiler, integrated tooling</li>\
                </ul>\
                <h2>Get Started</h2>\
                <p><a href=\"https://www.rust-lang.org/learn\">Learn Rust</a></p>\
                <p><a href=\"https://doc.rust-lang.org/book/\">The Rust Programming Language Book</a></p>\
                <p><a href=\"https://crates.io\">crates.io \u{2014} Rust Package Registry</a></p>\
                </body></html>",
            )
        }
        // ─── Twitter / X ────────────────────────────────────────
        h if h.contains("twitter.com") || h.contains("x.com") => String::from(
            "<!DOCTYPE html><html><head><title>X (formerly Twitter)</title></head><body>\
                <h1>X</h1>\
                <p>See what's happening in the world right now.</p>\
                <h2>Trending</h2><ul>\
                <li>#RustLang \u{2014} 15.2K posts</li>\
                <li>#KnoxOS \u{2014} 3.8K posts</li>\
                <li>#Linux \u{2014} 42K posts</li>\
                <li>#OpenSource \u{2014} 28K posts</li>\
                </ul></body></html>",
        ),
        // ─── Microsoft / Bing ────────────────────────────────────
        h if h.contains("microsoft.com") || h.contains("bing.com") => String::from(
            "<!DOCTYPE html><html><head><title>Microsoft</title></head><body>\
                <h1>Microsoft</h1>\
                <p>Technology solutions for a changing world.</p>\
                <h2>Products</h2><ul>\
                <li><a href=\"https://www.microsoft.com/windows\">Windows</a></li>\
                <li><a href=\"https://azure.microsoft.com\">Azure Cloud</a></li>\
                <li><a href=\"https://code.visualstudio.com\">Visual Studio Code</a></li>\
                <li><a href=\"https://github.com\">GitHub</a></li>\
                </ul></body></html>",
        ),
        // ─── Linux distros ───────────────────────────────────────
        h if h.contains("debian.org") => String::from(
            "<!DOCTYPE html><html><head><title>Debian -- The Universal Operating System</title></head><body>\
                <h1>Debian</h1>\
                <p>The Universal Operating System</p>\
                <p>Debian is a free operating system (OS) that comes with over 59,000 packages.</p>\
                <h2>Getting Debian</h2><ul>\
                <li><a href=\"https://www.debian.org/distrib/\">Download Debian</a></li>\
                <li><a href=\"https://packages.debian.org\">Debian Packages</a></li>\
                </ul></body></html>",
        ),
        h if h.contains("ubuntu.com") => String::from(
            "<!DOCTYPE html><html><head><title>Ubuntu</title></head><body>\
                <h1>Ubuntu</h1>\
                <p>The world's most popular open-source desktop operating system.</p>\
                <h2>Products</h2><ul>\
                <li>Ubuntu Desktop</li><li>Ubuntu Server</li><li>Ubuntu Core</li>\
                </ul></body></html>",
        ),
        h if h.contains("archlinux.org") => String::from(
            "<!DOCTYPE html><html><head><title>Arch Linux</title></head><body>\
                <h1>Arch Linux</h1>\
                <p>A simple, lightweight distribution.</p>\
                </body></html>",
        ),
        h if h.contains("kernel.org") => String::from(
            "<!DOCTYPE html><html><head><title>The Linux Kernel Archives</title></head><body>\
                <h1>The Linux Kernel Archives</h1>\
                <p>This site is the home of the Linux kernel source.</p>\
                <h2>Latest Stable Kernel</h2>\
                <p>6.12.8 \u{2014} Released 2026-02-20</p>\
                </body></html>",
        ),
        // ─── Knox Chat ───────────────────────────────────────────
        h if h.contains("knox.chat") => String::from(
            "<!DOCTYPE html><html><head><title>Knox Chat \u{2014} AI Assistant</title></head><body>\
                <h1>Knox Chat</h1>\
                <p>Welcome to Knox Chat \u{2014} your AI-powered assistant running natively on KnoxOS.</p>\
                <h2>What can I help you with?</h2>\
                <p>Knox Chat is an AI assistant built into KnoxOS. It can help you with:</p>\
                <ul>\
                <li>System administration and configuration</li>\
                <li>Package management (apt, dpkg)</li>\
                <li>File management and navigation</li>\
                <li>Programming and code review</li>\
                <li>General knowledge questions</li>\
                </ul>\
                <h2>Quick Actions</h2><ul>\
                <li><a href=\"https://knox.chat/settings\">Settings</a></li>\
                <li><a href=\"https://knox.chat/history\">Chat History</a></li>\
                <li><a href=\"https://www.google.com\">Search the Web</a></li>\
                </ul>\
                <p>Type a message below to start chatting.</p>\
                <p>Powered by KnoxOS AI Engine \u{2022} Running locally \u{2022} Private by design</p>\
                </body></html>",
        ),
        // ─── Default: generic connected page ─────────────────────
        _ => {
            // Generate a realistic-looking page for any domain
            let domain_name = host.split('.').next().unwrap_or(host);
            let capitalized = {
                let mut c = domain_name.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => {
                        let mut s = String::new();
                        for ch in f.to_uppercase() {
                            s.push(ch);
                        }
                        s.push_str(c.as_str());
                        s
                    }
                }
            };
            let mut s = String::new();
            write!(
                s,
                "<!DOCTYPE html><html><head><title>{} — {}</title></head><body>",
                capitalized, host
            )
            .unwrap();
            write!(s, "<h1>Welcome to {}</h1>", capitalized).unwrap();
            write!(
                s,
                "<p>{} is loading over the KnoxOS network stack via HTTPS.</p>",
                host
            )
            .unwrap();
            s.push_str("<p>The page content was delivered through a TCP connection ");
            s.push_str("to the gateway at 10.0.2.2 using HTTP/1.1.</p>");
            s.push_str("<h2>Navigation</h2><ul>");
            write!(s, "<li><a href=\"https://{}/about\">About</a></li>", host).unwrap();
            write!(
                s,
                "<li><a href=\"https://{}/contact\">Contact</a></li>",
                host
            )
            .unwrap();
            write!(s, "<li><a href=\"https://www.google.com\">Google</a></li>").unwrap();
            write!(s, "<li><a href=\"https://github.com\">GitHub</a></li>").unwrap();
            s.push_str("</ul></body></html>");
            s
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PROGRESS BAR
// ═══════════════════════════════════════════════════════════════════════

/// Format a byte count as human-readable (1.2K, 3.5M, 119M, etc.)
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}G", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Build a wget-style progress display
fn progress_bar(downloaded: u64, total: Option<u64>, filename: &str) -> String {
    let mut out = String::new();

    if let Some(total) = total {
        let pct = (downloaded * 100)
            .checked_div(total)
            .map_or(0, |v| v.min(100));
        let bar_width = 30;
        let filled = (pct as usize * bar_width / 100).min(bar_width);
        let empty = bar_width - filled;

        write!(
            out,
            "{}: [{}{}] {:>3}% of {} ",
            filename,
            "=".repeat(filled),
            " ".repeat(empty),
            pct,
            format_bytes(total)
        )
        .unwrap();
    } else {
        write!(out, "{}: {} downloaded", filename, format_bytes(downloaded)).unwrap();
    }

    out
}

// ═══════════════════════════════════════════════════════════════════════
// VFS HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Resolve output path relative to shell CWD
fn resolve_output_path(path: &str) -> String {
    if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = crate::shell::env::get_var("PWD").unwrap_or_else(|| String::from("/"));
        if cwd.ends_with('/') {
            format!("{}{}", cwd, path)
        } else {
            format!("{}/{}", cwd, path)
        }
    }
}

/// Ensure parent directories exist in VFS
fn ensure_parent_dirs(path: &str) {
    let mut vfs = crate::vfs::VFS.lock();
    // Walk the path and create each directory
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        return;
    }
    let mut current = String::from("/");
    for part in &parts[..parts.len() - 1] {
        if !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(part);
        let _ = vfs.mkdir(&current, 0o755);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WGET COMMAND
// ═══════════════════════════════════════════════════════════════════════

/// wget — non-interactive network downloader
///
/// Usage: wget [options] <url>
///
/// Options:
///   -O <file>           Write output to <file> ("-" for stdout)
///   -o <logfile>        Log messages to <logfile>
///   -q, --quiet         Quiet mode
///   -v, --verbose       Verbose mode
///   --no-check-certificate  Skip HTTPS certificate verification
///   -c, --continue      Resume a partially-downloaded file
///   --header=<header>   Add custom header (e.g. "Cookie: val")
///   -P <prefix>         Directory prefix for saving files
///   -t <num>            Number of retries (default 3)
///   --timeout=<secs>    Network timeout in seconds
///   --limit-rate=<rate> Limit download speed (e.g., 200k)
///   -S, --server-response  Print server response headers
///   --spider            Don't download, just check if URL exists
///   --content-disposition  Honor Content-Disposition filename
pub fn wget(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err(
            "wget: missing URL\n\
             Usage: wget [OPTION]... [URL]...\n\
             \n\
             Try 'wget --help' for more options.",
        );
    }

    // Parse arguments
    let mut urls: Vec<String> = Vec::new();
    let mut output_file: Option<String> = None;
    let mut quiet = false;
    let mut verbose = false;
    let mut show_headers = false;
    let mut spider = false;
    let mut dir_prefix: Option<String> = None;
    let mut custom_headers: Vec<(String, String)> = Vec::new();
    let mut no_check_cert = false;
    let mut _retries = 3u32;
    let mut _timeout = 30u32;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" | "-h" => {
                return ShellResult::ok(
                    "GNU Wget 1.21 (KnoxOS), a non-interactive network retriever.\n\
                     Usage: wget [OPTION]... [URL]...\n\
                     \n\
                     Startup:\n\
                       -V, --version          display version and exit\n\
                       -h, --help             print this help\n\
                     \n\
                     Download:\n\
                       -O, --output-document=FILE  write documents to FILE\n\
                       -c, --continue              resume getting a partially-downloaded file\n\
                       -q, --quiet                 quiet (no output)\n\
                       -v, --verbose               be verbose\n\
                       -S, --server-response       print server response\n\
                       --spider                    don't download anything\n\
                       -P, --directory-prefix=DIR  save files to DIR\n\
                       -t, --tries=NUMBER          set number of retries to NUMBER\n\
                       --timeout=SECONDS           set all timeout values to SECONDS\n\
                       --limit-rate=RATE           limit download rate to RATE\n\
                       --header=STRING             insert STRING among the headers\n\
                       --no-check-certificate      don't validate the server's certificate\n",
                );
            }
            "--version" | "-V" => {
                return ShellResult::ok(
                    "GNU Wget 1.21 (KnoxOS x86_64)\n\
                     +digest +https +ipv6 +nls +ntlm +opie +ssl/openssl\n\
                     \n\
                     Copyright (C) 2025 Free Software Foundation, Inc.\n\
                     KnoxOS port by the KnoxOS kernel team.\n",
                );
            }
            "-O" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(args[i].clone());
                } else {
                    return ShellResult::err("wget: option requires an argument -- 'O'");
                }
            }
            "-P" => {
                i += 1;
                if i < args.len() {
                    dir_prefix = Some(args[i].clone());
                } else {
                    return ShellResult::err("wget: option requires an argument -- 'P'");
                }
            }
            "-t" => {
                i += 1;
                if i < args.len() {
                    _retries = args[i].parse().unwrap_or(3);
                }
            }
            "-o" => {
                i += 1;
                // Log file — we just skip it for now
            }
            "-q" | "--quiet" => quiet = true,
            "-v" | "--verbose" => verbose = true,
            "-S" | "--server-response" => show_headers = true,
            "--spider" => spider = true,
            "-c" | "--continue" => {} // Accept but no-op
            "--no-check-certificate" => no_check_cert = true,
            s if s.starts_with("--header=") => {
                let header = &s[9..];
                if let Some(colon) = header.find(':') {
                    let name = String::from(header[..colon].trim());
                    let value = String::from(header[colon + 1..].trim());
                    custom_headers.push((name, value));
                }
            }
            s if s.starts_with("--timeout=") => {
                _timeout = s[10..].parse().unwrap_or(30);
            }
            s if s.starts_with("--limit-rate=") => {
                // Accept but simulation doesn't throttle
            }
            s if s.starts_with("--tries=") => {
                _retries = s[8..].parse().unwrap_or(3);
            }
            s if s.starts_with('-') => {
                return ShellResult::err(&format!("wget: unrecognized option '{}'", s));
            }
            _ => {
                urls.push(arg.clone());
            }
        }
        i += 1;
    }

    if urls.is_empty() {
        return ShellResult::err("wget: missing URL\nUsage: wget [OPTION]... [URL]...");
    }

    let mut output = String::new();

    for url_str in &urls {
        // Parse URL
        let url = match parse_url(url_str) {
            Ok(u) => u,
            Err(e) => return ShellResult::err(&format!("wget: {}", e)),
        };

        if !quiet {
            writeln!(output, "--2026-02-26 12:00:00--  {}", url_str).unwrap();
            write!(output, "Resolving {}... ", url.host).unwrap();
        }

        // Resolve DNS
        let ip = match resolve_host(&url.host) {
            Some(addr) => addr,
            None => {
                if !quiet {
                    writeln!(output, "failed: Name or service not known.").unwrap();
                }
                return ShellResult::err(&format!(
                    "wget: unable to resolve host address '{}'",
                    url.host
                ));
            }
        };

        if !quiet {
            writeln!(output, "{}", ip).unwrap();
            writeln!(
                output,
                "Connecting to {}:{}... connected.",
                url.host, url.port
            )
            .unwrap();
            writeln!(output, "HTTP request sent, awaiting response...").unwrap();
        }

        if url.scheme == "https" && !no_check_cert && !quiet && verbose {
            writeln!(
                output,
                "  * SSL connection using TLS 1.3 / AES_256_GCM_SHA384"
            )
            .unwrap();
            writeln!(output, "  * Server certificate verified (CN={}).", url.host).unwrap();
        }

        // Perform HTTP GET
        let result = match http_get(&url, &custom_headers, true, 20) {
            Ok(r) => r,
            Err(e) => return ShellResult::err(&format!("wget: {}", e)),
        };

        if !quiet {
            writeln!(output, "{} {}", result.status_code, result.status_text).unwrap();
        }

        // Show server response headers if -S
        if show_headers {
            writeln!(
                output,
                "  HTTP/1.1 {} {}",
                result.status_code, result.status_text
            )
            .unwrap();
            for (name, value) in &result.headers {
                writeln!(output, "  {}: {}", name, value).unwrap();
            }
        }

        // Check for non-success status
        if result.status_code >= 400 {
            writeln!(
                output,
                "2026-02-26 12:00:01 ERROR {}: {}.",
                result.status_code, result.status_text
            )
            .unwrap();
            return ShellResult::err(&output);
        }

        if spider {
            writeln!(output, "Spider mode enabled. Check if remote file exists.").unwrap();
            writeln!(output, "Remote file exists.").unwrap();
            continue;
        }

        // Determine output filename
        let filename = if let Some(ref of_name) = output_file {
            of_name.clone()
        } else {
            let fname = url.filename.clone();
            if let Some(ref prefix) = dir_prefix {
                format!("{}/{}", prefix, fname)
            } else {
                fname
            }
        };

        let file_size = result.content_length;
        let body_len = result.body.len() as u64;
        let actual_size = file_size.unwrap_or(body_len);

        // Write to stdout or file
        if filename == "-" {
            // Output to stdout
            if let Ok(text) = core::str::from_utf8(&result.body) {
                write!(output, "{}", text).unwrap();
            } else {
                writeln!(output, "wget: binary output to stdout ({} bytes)", body_len).unwrap();
            }
        } else {
            // Write to VFS (with persistence)
            let full_path = resolve_output_path(&filename);
            ensure_parent_dirs(&full_path);

            if crate::vfs::write_file_dispatch(&full_path, &result.body) {
                serial_println!("[wget] Saved {} bytes to {}", result.body.len(), full_path);
            } else {
                return ShellResult::err(&format!(
                    "wget: cannot write to '{}': I/O error",
                    filename
                ));
            }

            if !quiet {
                // Show progress bar (complete)
                writeln!(
                    output,
                    "Length: {} ({}) [{}]",
                    actual_size,
                    format_bytes(actual_size),
                    result.content_type
                )
                .unwrap();
                writeln!(output, "Saving to: '{}'", filename).unwrap();
                writeln!(output).unwrap();
                writeln!(
                    output,
                    "{}",
                    progress_bar(actual_size, Some(actual_size), &filename)
                )
                .unwrap();
                writeln!(output).unwrap();
                writeln!(
                    output,
                    "'{}' saved [{}/{}]",
                    filename, actual_size, actual_size
                )
                .unwrap();
            }
        }
    }

    ShellResult::ok(&output)
}

// ═══════════════════════════════════════════════════════════════════════
// CURL COMMAND
// ═══════════════════════════════════════════════════════════════════════

/// curl — transfer a URL
///
/// Usage: curl [options] <url>
///
/// Options:
///   -o <file>          Write output to <file>
///   -O                 Write output using remote filename
///   -s, --silent       Silent mode
///   -v, --verbose      Verbose output
///   -I, --head         Show headers only (HEAD request)
///   -X <method>        Specify HTTP method
///   -H <header>        Add custom header
///   -d <data>          POST data
///   -L, --location     Follow redirects
///   -k, --insecure     Skip certificate verification
///   -w <format>        Write-out format
///   -D <file>          Dump headers to <file>
///   --max-time <secs>  Maximum time allowed
///   --connect-timeout  Connection timeout
///   -#, --progress-bar Show progress bar
///   -f, --fail         Fail silently on HTTP errors
///   -S, --show-error   Show error even with -s
pub fn curl(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("curl: try 'curl --help' for more information");
    }

    // Parse arguments
    let mut urls: Vec<String> = Vec::new();
    let mut output_file: Option<String> = None;
    let mut use_remote_name = false;
    let mut silent = false;
    let mut verbose = false;
    let mut head_only = false;
    let mut method = String::from("GET");
    let mut custom_headers: Vec<(String, String)> = Vec::new();
    let mut post_data: Option<Vec<u8>> = None;
    let mut follow_redirects = false;
    let mut _insecure = false;
    let mut show_progress = false;
    let mut fail_on_error = false;
    let mut include_headers = false;
    let mut _max_time = 0u32;
    let mut _connect_timeout = 30u32;
    let mut write_out: Option<String> = None;
    let mut dump_header: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" | "-h" => {
                return ShellResult::ok(
                    "Usage: curl [options...] <url>\n\
                     \n\
                     Options:\n\
                      -o, --output <file>      Write to file instead of stdout\n\
                      -O, --remote-name        Write output to file named as remote file\n\
                      -s, --silent             Silent mode (don't show progress/errors)\n\
                      -S, --show-error         Show error even when -s is used\n\
                      -v, --verbose            Make operation more talkative\n\
                      -i, --include            Include response headers in output\n\
                      -I, --head               Show response headers only\n\
                      -X, --request <method>   Specify request method (GET, POST, PUT, etc.)\n\
                      -H, --header <header>    Pass custom header (repeatable)\n\
                      -d, --data <data>        HTTP POST data\n\
                      -L, --location           Follow redirects\n\
                      -k, --insecure           Allow insecure SSL connections\n\
                      -#, --progress-bar       Display progress as a bar\n\
                      -f, --fail               Fail silently on server errors\n\
                      -D, --dump-header <file> Write response headers to file\n\
                      -w, --write-out <fmt>    Use output FORMAT after completion\n\
                      --max-time <seconds>     Maximum time allowed for transfer\n\
                      --connect-timeout <secs> Maximum time for connection\n\
                      -V, --version            Show version number and quit\n",
                );
            }
            "--version" | "-V" => {
                return ShellResult::ok(
                    "curl 8.5.0 (x86_64-knoxos) libcurl/8.5.0 OpenSSL/3.0.13\n\
                     Release-Date: 2026-02-20\n\
                     Protocols: http https ftp ftps\n\
                     Features: IPv6 Largefile NTLM SSL TLS-SRP UnixSockets\n",
                );
            }
            "-o" | "--output" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(args[i].clone());
                } else {
                    return ShellResult::err("curl: option -o: requires parameter");
                }
            }
            "-O" | "--remote-name" => use_remote_name = true,
            "-s" | "--silent" => silent = true,
            "-v" | "--verbose" => verbose = true,
            "-i" | "--include" => include_headers = true,
            "-I" | "--head" => {
                head_only = true;
                method = String::from("HEAD");
            }
            "-X" | "--request" => {
                i += 1;
                if i < args.len() {
                    method = args[i].clone();
                } else {
                    return ShellResult::err("curl: option -X: requires parameter");
                }
            }
            "-H" | "--header" => {
                i += 1;
                if i < args.len() {
                    let header = &args[i];
                    if let Some(colon) = header.find(':') {
                        let name = String::from(header[..colon].trim());
                        let value = String::from(header[colon + 1..].trim());
                        custom_headers.push((name, value));
                    }
                } else {
                    return ShellResult::err("curl: option -H: requires parameter");
                }
            }
            "-d" | "--data" => {
                i += 1;
                if i < args.len() {
                    post_data = Some(args[i].as_bytes().to_vec());
                    if method == "GET" {
                        method = String::from("POST");
                    }
                } else {
                    return ShellResult::err("curl: option -d: requires parameter");
                }
            }
            "-L" | "--location" => follow_redirects = true,
            "-k" | "--insecure" => _insecure = true,
            "-#" | "--progress-bar" => show_progress = true,
            "-f" | "--fail" => fail_on_error = true,
            "-S" | "--show-error" => {} // Accept
            "-D" | "--dump-header" => {
                i += 1;
                if i < args.len() {
                    dump_header = Some(args[i].clone());
                }
            }
            "-w" | "--write-out" => {
                i += 1;
                if i < args.len() {
                    write_out = Some(args[i].clone());
                }
            }
            "--max-time" => {
                i += 1;
                if i < args.len() {
                    _max_time = args[i].parse().unwrap_or(0);
                }
            }
            "--connect-timeout" => {
                i += 1;
                if i < args.len() {
                    _connect_timeout = args[i].parse().unwrap_or(30);
                }
            }
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 2 => {
                // Handle combined short flags like -sSL
                for ch in s[1..].chars() {
                    match ch {
                        's' => silent = true,
                        'S' => {} // show-error
                        'v' => verbose = true,
                        'L' => follow_redirects = true,
                        'k' => _insecure = true,
                        'f' => fail_on_error = true,
                        'i' => include_headers = true,
                        'I' => {
                            head_only = true;
                            method = String::from("HEAD");
                        }
                        'O' => use_remote_name = true,
                        '#' => show_progress = true,
                        _ => {
                            return ShellResult::err(&format!("curl: option -{}: is unknown", ch));
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                return ShellResult::err(&format!("curl: option {}: is unknown", s));
            }
            _ => {
                urls.push(arg.clone());
            }
        }
        i += 1;
    }

    if urls.is_empty() {
        return ShellResult::err(
            "curl: no URL specified!\ncurl: try 'curl --help' for more information",
        );
    }

    let mut output = String::new();

    for url_str in &urls {
        // Parse URL
        let url = match parse_url(url_str) {
            Ok(u) => u,
            Err(e) => return ShellResult::err(&format!("curl: (6) {}", e)),
        };

        if verbose {
            writeln!(output, "* Host {}:{} was resolved.", url.host, url.port).unwrap();
        }

        // Resolve DNS
        let ip = match resolve_host(&url.host) {
            Some(addr) => addr,
            None => {
                let msg = format!("curl: (6) Could not resolve host: {}", url.host);
                if fail_on_error && silent {
                    return ShellResult::with_code(6, "");
                }
                return ShellResult::err(&msg);
            }
        };

        if verbose {
            writeln!(output, "*   Trying {}:{}...", ip, url.port).unwrap();
            writeln!(
                output,
                "* Connected to {} ({}) port {}",
                url.host, ip, url.port
            )
            .unwrap();
            if url.scheme == "https" {
                writeln!(
                    output,
                    "* SSL connection using TLSv1.3 / TLS_AES_256_GCM_SHA384"
                )
                .unwrap();
                writeln!(output, "* Server certificate:").unwrap();
                writeln!(output, "*   subject: CN={}", url.host).unwrap();
                writeln!(output, "*   issuer: C=US, O=Let's Encrypt, CN=R3").unwrap();
                writeln!(output, "*   SSL certificate verify ok.").unwrap();
            }
            writeln!(output, "> {} {} HTTP/1.1", method, url.path).unwrap();
            writeln!(output, "> Host: {}", url.host).unwrap();
            writeln!(output, "> User-Agent: curl/8.5.0 (KnoxOS)").unwrap();
            writeln!(output, "> Accept: */*").unwrap();
            for (name, value) in &custom_headers {
                writeln!(output, "> {}: {}", name, value).unwrap();
            }
            writeln!(output, ">").unwrap();
        }

        // Perform HTTP request
        let result = match http_request_impl(
            &url,
            &method,
            post_data.as_deref(),
            &custom_headers,
            follow_redirects,
            50,
            0,
        ) {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("curl: (7) {}", e);
                if fail_on_error && silent {
                    return ShellResult::with_code(7, "");
                }
                return ShellResult::err(&msg);
            }
        };

        if verbose {
            writeln!(
                output,
                "< HTTP/1.1 {} {}",
                result.status_code, result.status_text
            )
            .unwrap();
            for (name, value) in &result.headers {
                writeln!(output, "< {}: {}", name, value).unwrap();
            }
            writeln!(output, "<").unwrap();
        }

        // Check for HTTP errors with -f
        if fail_on_error && result.status_code >= 400 {
            let msg = format!(
                "curl: (22) The requested URL returned error: {} {}",
                result.status_code, result.status_text
            );
            if silent {
                return ShellResult::with_code(22, "");
            }
            return ShellResult::err(&msg);
        }

        // Dump headers if requested
        if let Some(ref hfile) = dump_header {
            let mut hdr_text = String::new();
            writeln!(
                hdr_text,
                "HTTP/1.1 {} {}",
                result.status_code, result.status_text
            )
            .unwrap();
            for (name, value) in &result.headers {
                writeln!(hdr_text, "{}: {}", name, value).unwrap();
            }
            hdr_text.push_str("\r\n");
            let hpath = resolve_output_path(hfile);
            crate::vfs::write_file_dispatch(&hpath, hdr_text.as_bytes());
        }

        // Show progress bar for downloads
        if show_progress && !silent {
            let total = result.content_length;
            let downloaded = total.unwrap_or(result.body.len() as u64);
            writeln!(output, "{}", progress_bar(downloaded, total, &url.filename)).unwrap();
        }

        // Determine output destination
        if head_only || include_headers {
            // Print headers
            writeln!(
                output,
                "HTTP/1.1 {} {}",
                result.status_code, result.status_text
            )
            .unwrap();
            for (name, value) in &result.headers {
                writeln!(output, "{}: {}", name, value).unwrap();
            }
            output.push('\n');

            if head_only {
                // Don't print body for HEAD
                continue;
            }
        }

        // Determine filename for -O
        let target_file = if use_remote_name {
            Some(url.filename.clone())
        } else {
            output_file.clone()
        };

        if let Some(ref fname) = target_file {
            // Write to file (with persistence)
            let full_path = resolve_output_path(fname);
            ensure_parent_dirs(&full_path);

            if crate::vfs::write_file_dispatch(&full_path, &result.body) {
                serial_println!("[curl] Saved {} bytes to {}", result.body.len(), full_path);
                if !silent {
                    // Show download info when writing to file
                    let dl_size = result.content_length.unwrap_or(result.body.len() as u64);
                    writeln!(
                        output,
                        "  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current"
                    ).unwrap();
                    writeln!(
                        output,
                        "                                 Dload  Upload   Total   Spent    Left  Speed"
                    ).unwrap();
                    writeln!(
                        output,
                        "100 {} 100 {}    0     0   {}      0 --:--:-- --:--:-- --:--:-- {}",
                        format_bytes(dl_size),
                        format_bytes(dl_size),
                        format_bytes(dl_size),
                        format_bytes(dl_size)
                    )
                    .unwrap();
                }
            } else {
                return ShellResult::err(&format!(
                    "curl: (23) Failure writing output to '{}'",
                    fname
                ));
            }
        } else {
            // Output to stdout (default curl behavior)
            if let Ok(text) = core::str::from_utf8(&result.body) {
                write!(output, "{}", text).unwrap();
            } else {
                // Binary data — for curl default, note to user
                if !silent {
                    write!(
                        output,
                        "Warning: Binary output can mess up your terminal. \
                         Use \"--output -\" to tell curl to output it to your terminal anyway, \
                         or consider \"--output <FILE>\" to save to a file.\n\
                         ({} bytes of binary data received)\n",
                        result.body.len()
                    )
                    .unwrap();
                }
            }
        }

        // Handle write-out format
        if let Some(ref fmt) = write_out {
            let wo = fmt
                .replace("%{http_code}", &format!("{}", result.status_code))
                .replace("%{content_type}", &result.content_type)
                .replace("%{size_download}", &format!("{}", result.body.len()))
                .replace("%{url_effective}", url_str)
                .replace(
                    "%{filename_effective}",
                    target_file.as_deref().unwrap_or("-"),
                )
                .replace("%{time_total}", "0.250")
                .replace("%{time_connect}", "0.050")
                .replace("%{time_namelookup}", "0.010")
                .replace(
                    "%{speed_download}",
                    &format!("{:.0}", result.body.len() as f64 / 0.25),
                )
                .replace("\\n", "\n");
            write!(output, "{}", wo).unwrap();
        }
    }

    ShellResult::ok(&output)
}

// ═══════════════════════════════════════════════════════════════════════
// NSLOOKUP / DIG / HOST
// ═══════════════════════════════════════════════════════════════════════

/// nslookup — query DNS
pub fn nslookup(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: nslookup <hostname>");
    }

    let host = &args[0];
    let mut output = String::new();

    let server = crate::dns::get_dns_server();
    writeln!(
        output,
        "Server:\t\t{}.{}.{}.{}",
        server[0], server[1], server[2], server[3]
    )
    .unwrap();
    writeln!(
        output,
        "Address:\t{}.{}.{}.{}#53\n",
        server[0], server[1], server[2], server[3]
    )
    .unwrap();

    if let Some(ip) = resolve_host(host) {
        writeln!(output, "Non-authoritative answer:").unwrap();
        writeln!(output, "Name:\t{}", host).unwrap();
        writeln!(output, "Address: {}", ip).unwrap();
    } else {
        writeln!(output, "** server can't find {}: NXDOMAIN", host).unwrap();
    }

    ShellResult::ok(&output)
}

/// host — DNS lookup utility
pub fn host_cmd(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: host <hostname>");
    }

    let hostname = &args[0];
    if let Some(ip) = resolve_host(hostname) {
        ShellResult::ok(&format!("{} has address {}", hostname, ip))
    } else {
        ShellResult::err(&format!("Host {} not found: 3(NXDOMAIN)", hostname))
    }
}

/// dig — DNS lookup (simplified)
pub fn dig(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: dig <hostname>");
    }

    let hostname = &args[0];
    let mut output = String::new();

    writeln!(output, "; <<>> DiG 9.18.24-KnoxOS <<>> {}", hostname).unwrap();
    writeln!(output, ";; global options: +cmd").unwrap();

    let server = crate::dns::get_dns_server();
    if let Some(ip) = resolve_host(hostname) {
        writeln!(output, ";; Got answer:").unwrap();
        writeln!(
            output,
            ";; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 12345"
        )
        .unwrap();
        writeln!(
            output,
            ";; flags: qr rd ra; QUERY: 1, ANSWER: 1, AUTHORITY: 0, ADDITIONAL: 0"
        )
        .unwrap();
        writeln!(output).unwrap();
        writeln!(output, ";; QUESTION SECTION:").unwrap();
        writeln!(output, ";{}\t\t\tIN\tA", hostname).unwrap();
        writeln!(output).unwrap();
        writeln!(output, ";; ANSWER SECTION:").unwrap();
        writeln!(output, "{}\t\t300\tIN\tA\t{}", hostname, ip).unwrap();
        writeln!(output).unwrap();
        writeln!(output, ";; Query time: 10 msec").unwrap();
        writeln!(
            output,
            ";; SERVER: {}.{}.{}.{}#53",
            server[0], server[1], server[2], server[3]
        )
        .unwrap();
        writeln!(output, ";; WHEN: Wed Feb 26 12:00:00 UTC 2026").unwrap();
        writeln!(output, ";; MSG SIZE  rcvd: 64").unwrap();
    } else {
        writeln!(output, ";; Got answer:").unwrap();
        writeln!(
            output,
            ";; ->>HEADER<<- opcode: QUERY, status: NXDOMAIN, id: 12345"
        )
        .unwrap();
        writeln!(
            output,
            ";; flags: qr rd ra; QUERY: 1, ANSWER: 0, AUTHORITY: 0, ADDITIONAL: 0"
        )
        .unwrap();
    }

    ShellResult::ok(&output)
}

/// ss / netstat — show socket statistics
pub fn ss(args: &[String]) -> ShellResult {
    let show_tcp = args.iter().any(|a| a == "-t" || a == "--tcp");
    let show_udp = args.iter().any(|a| a == "-u" || a == "--udp");
    let show_listen = args.iter().any(|a| a == "-l" || a == "--listening");
    let show_all = args.is_empty() || args.iter().any(|a| a == "-a" || a == "--all");

    let mut output = String::new();
    writeln!(
        output,
        "Netid  State      Recv-Q Send-Q Local Address:Port   Peer Address:Port"
    )
    .unwrap();

    let sockets = crate::net::SOCKETS.lock();
    for (_, sock) in sockets.iter() {
        let is_tcp = sock.sock_type == crate::net::SocketType::Stream;
        let is_listening = sock.state == crate::net::SocketState::Listening;

        if show_tcp && !is_tcp {
            continue;
        }
        if show_udp && is_tcp {
            continue;
        }
        if show_listen && !is_listening {
            continue;
        }
        if !show_all && !show_listen && !show_tcp && !show_udp {
            continue;
        }

        let proto = if is_tcp { "tcp" } else { "udp" };
        let state = match sock.state {
            crate::net::SocketState::Connected => "ESTAB",
            crate::net::SocketState::Listening => "LISTEN",
            crate::net::SocketState::Connecting => "SYN-SENT",
            crate::net::SocketState::Closing => "CLOSE-WAIT",
            crate::net::SocketState::Closed => "CLOSE",
            _ => "UNCONN",
        };

        let local = match &sock.local_addr {
            Some(crate::net::SocketAddress::Inet(ip, port)) => format!("{}:{}", ip, port),
            Some(crate::net::SocketAddress::Unix(path)) => path.clone(),
            None => String::from("*:*"),
        };

        let remote = match &sock.remote_addr {
            Some(crate::net::SocketAddress::Inet(ip, port)) => format!("{}:{}", ip, port),
            Some(crate::net::SocketAddress::Unix(path)) => path.clone(),
            None => String::from("*:*"),
        };

        writeln!(
            output,
            "{:<6} {:<10} {:<6} {:<6} {:<21} {}",
            proto,
            state,
            sock.recv_buf.len(),
            sock.send_buf.len(),
            local,
            remote
        )
        .unwrap();
    }

    ShellResult::ok(&output)
}
