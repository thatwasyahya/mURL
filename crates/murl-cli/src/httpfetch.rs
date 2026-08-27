//! The HTTPS manifest fetcher.
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
//!   email must not become a free request into the victim's LAN.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use murl_core::error::{Error, Result};
use murl_core::fetch::RemoteFetcher;
use murl_core::limits::Limits;

use crate::logger;

#[derive(Debug, Default)]
pub struct HttpsFetcher;

impl RemoteFetcher for HttpsFetcher {
    fn fetch(&self, url: &str, limits: &Limits) -> Result<Vec<u8>> {
        let loopback = url_host(url).map(host_is_loopback).unwrap_or(false);
        if !(url.starts_with("https://") || (url.starts_with("http://") && loopback)) {
            return Err(Error::Fetch(format!(
                "refusing non-HTTPS manifest URL `{url}` (http is allowed for loopback only)"
            )));
        }

        logger::debug(&format!("fetching {url}"));
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(limits.fetch_timeout_secs))
            .redirects(limits.max_redirects)
            .resolver(GuardedResolver {
                allow_loopback: loopback,
            })
            .user_agent(concat!("murl/", env!("CARGO_PKG_VERSION")))
            .build();

        match agent
            .get(url)
            .set("Accept", "application/murl+json, application/json")
            .call()
        {
            Ok(resp) => read_capped(resp, limits.max_manifest_bytes, url),
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
#[derive(Debug)]
struct GuardedResolver {
    allow_loopback: bool,
}

impl ureq::Resolver for GuardedResolver {
    fn resolve(&self, netloc: &str) -> std::io::Result<Vec<SocketAddr>> {
        use std::net::ToSocketAddrs;
        let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
        let total = addrs.len();
        let allowed: Vec<SocketAddr> = addrs
            .into_iter()
            .filter(|a| ip_allowed(&a.ip(), self.allow_loopback))
            .collect();
        if allowed.is_empty() && total > 0 {
            return Err(std::io::Error::other(format!(
                "`{netloc}` resolves only to blocked address ranges (private/link-local/loopback); \
                 manifest resolution refuses to reach into local networks"
            )));
        }
        Ok(allowed)
    }
}

fn ip_allowed(ip: &IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return allow_loopback;
            }
            !(v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                // Carrier-grade NAT 100.64.0.0/10 — still "someone's inside".
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64))
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return allow_loopback;
            }
            let seg0 = v6.segments()[0];
            !(v6.is_unspecified()
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
    fn url_host_extraction() {
        assert_eq!(url_host("https://example.com/a/b"), Some("example.com"));
        assert_eq!(url_host("http://localhost:8080/x"), Some("localhost"));
        assert_eq!(url_host("https://example.com"), Some("example.com"));
        assert_eq!(url_host("ftp://x"), None);
    }

    #[test]
    fn refuses_non_loopback_http() {
        let f = HttpsFetcher;
        let err = f
            .fetch("http://example.com/x", &Limits::default())
            .unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
    }
}
