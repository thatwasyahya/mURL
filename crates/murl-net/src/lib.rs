//! # murl-net
//!
//! The HTTPS manifest fetcher — the one place mURL talks to the network.
//! It lives in its own crate so the CLI and the daemon share a single
//! hardened implementation rather than each growing their own.
//!
//! Enforces, per the specification and threat model:
//!
//! * `https://` only — plain `http://` is accepted solely for loopback hosts
//!   (development), and the mURL grammar is what produced the URL.
//! * **Zero redirects.** A manifest lives where its authority's well-known
//!   path says it lives; following redirects would let one authority serve
//!   another authority's namespace.
//! * **Size cap enforced while reading**, not after: a 2 GB response costs
//!   us 256 KiB + 1 byte of buffer before it is rejected.
//! * **Address-range filtering at DNS resolution time**: for non-loopback
//!   URLs, names that resolve to private, link-local, loopback, or otherwise
//!   special ranges are refused. An OS-registered scheme handler is an
//!   SSRF-shaped primitive — `murl://printer.internal/...` arriving in an
//!   email must not become a free request into the victim's LAN. The
//!   converse holds for loopback authorities: they are the only ones fetched
//!   over plain http, so they may reach loopback addresses and nothing else.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use murl_core::error::{Error, Result};
use murl_core::fetch::RemoteFetcher;
use murl_core::limits::Limits;

/// Optional trace sink, so an embedder can log fetches without this crate
/// depending on a logging framework.
pub type TraceFn = fn(&str);

#[derive(Debug, Default)]
pub struct HttpsFetcher {
    trace: Option<TraceFn>,
}

impl HttpsFetcher {
    /// A fetcher that traces each request through `trace`.
    pub fn with_trace(trace: TraceFn) -> HttpsFetcher {
        HttpsFetcher { trace: Some(trace) }
    }

    fn trace(&self, message: &str) {
        if let Some(f) = self.trace {
            f(message);
        }
    }
}

impl RemoteFetcher for HttpsFetcher {
    fn fetch(&self, url: &str, limits: &Limits) -> Result<Vec<u8>> {
        let loopback = url_host(url).map(host_is_loopback).unwrap_or(false);
        let https = url.starts_with("https://");
        if !(https || (url.starts_with("http://") && loopback)) {
            return Err(Error::Fetch(format!(
                "refusing non-HTTPS manifest URL `{url}` (http is allowed for loopback only)"
            )));
        }

        self.trace(&format!("fetching {url}"));
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(limits.fetch_timeout_secs))
            .redirects(limits.max_redirects)
            // Redirects are off unless config turns them on; even then, an
            // https fetch must not be downgraded to plain http.
            .https_only(https)
            .resolver(GuardedResolver { loopback })
            .user_agent(concat!("murl/", env!("CARGO_PKG_VERSION")))
            .build();

        match agent
            .get(url)
            .set("Accept", "application/murl+json, application/json")
            .call()
        {
            // With redirects off, ureq hands a 3xx back as a response. It is
            // not a manifest, and neither is anything else outside 2xx.
            Ok(resp) => match resp.status() {
                200..=299 => read_capped(resp, limits.max_manifest_bytes, url),
                code => Err(Error::Fetch(format!("HTTP {code} from {url}"))),
            },
            Err(ureq::Error::Status(code, _)) => {
                Err(Error::Fetch(format!("HTTP {code} from {url}")))
            }
            Err(e) => Err(Error::Fetch(format!("{url}: {e}"))),
        }
    }
}

fn read_capped(resp: ureq::Response, cap: usize, url: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut reader = resp.into_reader().take(cap as u64 + 1);
    reader
        .read_to_end(&mut buf)
        .map_err(|e| Error::Fetch(format!("reading {url}: {e}")))?;
    if buf.len() > cap {
        return Err(Error::LimitExceeded(format!(
            "manifest at {url} exceeds {cap} bytes"
        )));
    }
    Ok(buf)
}

fn url_host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    Some(authority.rsplit_once(':').map_or(authority, |(h, _)| h))
}

fn host_is_loopback(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host.ends_with(".localhost")
}

/// DNS resolver wrapper that filters resolved addresses.
///
/// `loopback` is set for a loopback authority (`localhost`, `*.localhost`,
/// `127.0.0.1`), which may reach loopback addresses only: it is fetched over
/// plain http, so a hosts-file entry or a resolver that points
/// `dev.localhost` elsewhere must not turn it into an unencrypted request
/// across the network. Any other authority may reach public addresses only.
#[derive(Debug)]
struct GuardedResolver {
    loopback: bool,
}

impl ureq::Resolver for GuardedResolver {
    fn resolve(&self, netloc: &str) -> std::io::Result<Vec<SocketAddr>> {
        use std::net::ToSocketAddrs;
        let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
        let total = addrs.len();
        let allowed: Vec<SocketAddr> = addrs
            .into_iter()
            .filter(|a| ip_allowed(&a.ip(), self.loopback))
            .collect();
        if allowed.is_empty() && total > 0 {
            return Err(std::io::Error::other(if self.loopback {
                format!(
                    "`{netloc}` is a loopback name but resolves to non-loopback addresses; \
                     plain-http manifest fetches never leave this machine"
                )
            } else {
                format!(
                    "`{netloc}` resolves only to blocked address ranges (private/link-local/loopback); \
                     manifest resolution refuses to reach into local networks"
                )
            }));
        }
        Ok(allowed)
    }
}

fn ip_allowed(ip: &IpAddr, loopback: bool) -> bool {
    // `::ffff:10.0.0.1` reaches 10.0.0.1 on a dual-stack socket, so an
    // IPv4-mapped address is judged as the IPv4 address it carries.
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(*ip, IpAddr::V4),
        IpAddr::V4(_) => *ip,
    };
    if loopback {
        return ip.is_loopback();
    }
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                // "This network", 0.0.0.0/8, unspecified address included.
                || v4.octets()[0] == 0
                // Carrier-grade NAT 100.64.0.0/10 — still "someone's inside".
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64))
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique-local fc00::/7 and link-local fe80::/10.
                || (seg0 & 0xfe00) == 0xfc00
                || (seg0 & 0xffc0) == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_ranges() {
        for bad in [
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.1.1",
            "0.0.0.0",
        ] {
            let ip: IpAddr = bad.parse().unwrap();
            assert!(!ip_allowed(&ip, false), "{bad} should be blocked");
        }
        assert!(!ip_allowed(&"127.0.0.1".parse().unwrap(), false));
        assert!(ip_allowed(&"127.0.0.1".parse().unwrap(), true));
        assert!(ip_allowed(&"93.184.216.34".parse().unwrap(), false));
        assert!(!ip_allowed(&"fe80::1".parse().unwrap(), false));
        assert!(!ip_allowed(&"fc00::1".parse().unwrap(), false));
        assert!(!ip_allowed(&"::1".parse().unwrap(), false));
        assert!(ip_allowed(
            &"2606:2800:220:1:248:1893:25c8:1946".parse().unwrap(),
            false
        ));
    }

    #[test]
    fn blocks_special_ranges_in_disguise() {
        for bad in [
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
            "::ffff:169.254.169.254",
            "::ffff:127.0.0.1",
            "0.1.2.3",
        ] {
            let ip: IpAddr = bad.parse().unwrap();
            assert!(!ip_allowed(&ip, false), "{bad} should be blocked");
        }
        assert!(ip_allowed(&"::ffff:93.184.216.34".parse().unwrap(), false));
    }

    #[test]
    fn loopback_names_reach_loopback_only() {
        for ok in ["127.0.0.1", "127.0.0.2", "::1", "::ffff:127.0.0.1"] {
            let ip: IpAddr = ok.parse().unwrap();
            assert!(ip_allowed(&ip, true), "{ok} should be allowed");
        }
        for bad in ["93.184.216.34", "10.0.0.1", "2606:2800:220:1::1"] {
            let ip: IpAddr = bad.parse().unwrap();
            assert!(!ip_allowed(&ip, true), "{bad} should be blocked");
        }
    }

    /// Serves one canned HTTP response on a loopback port.
    fn serve_once(response: &'static str) -> u16 {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        port
    }

    #[test]
    fn a_redirect_is_not_a_manifest() {
        let port = serve_once(
            "HTTP/1.1 302 Found\r\nLocation: https://example.com/m.json\r\n\
             Content-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        let err = HttpsFetcher::default()
            .fetch(
                &format!("http://127.0.0.1:{port}/.well-known/murl/x.murl.json"),
                &Limits::default(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("HTTP 302"), "{err}");
    }

    #[test]
    fn a_2xx_body_is_returned() {
        let port =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        let body = HttpsFetcher::default()
            .fetch(
                &format!("http://127.0.0.1:{port}/.well-known/murl/x.murl.json"),
                &Limits::default(),
            )
            .unwrap();
        assert_eq!(body, b"{}");
    }

    #[test]
    fn url_host_extraction() {
        assert_eq!(url_host("https://example.com/a/b"), Some("example.com"));
        assert_eq!(url_host("http://localhost:8080/x"), Some("localhost"));
        assert_eq!(url_host("https://example.com"), Some("example.com"));
        assert_eq!(url_host("ftp://x"), None);
    }

    #[test]
    fn refuses_non_loopback_http() {
        let f = HttpsFetcher::default();
        let err = f
            .fetch("http://example.com/x", &Limits::default())
            .unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
    }
}
