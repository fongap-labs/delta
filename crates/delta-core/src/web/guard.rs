//! Address guard for URLs the model chooses.
//!
//! The model reads web pages, e-mail and chat messages, which are data and never instructions.
//! A page that talks the agent into fetching `http://169.254.169.254/` or
//! `http://127.0.0.1:11434/` would turn a read-only research tool into a probe of the machine's
//! own network position. This blocks the ranges that are reachable only because Delta runs on the
//! user's computer: loopback, private space, link-local (which includes the cloud metadata
//! address), shared carrier-grade NAT space, multicast and the reserved blocks, including an IPv4
//! address carried inside an IPv6 one.
//!
//! Every redirect hop is vetted, not just the first, and the connection is pinned to the address
//! that passed (see `fetch`), so a name that changes its answer between the check and the connect
//! changes nothing.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use url::{Host, Url};

/// Test-only relaxations. The product always uses [`AddressPolicy::STRICT`].
#[derive(Debug, Clone, Copy)]
pub struct AddressPolicy {
    /// Allow 127.0.0.0/8 and ::1, so tests can talk to a local server. Nothing else is relaxed.
    pub allow_loopback: bool,
}

impl AddressPolicy {
    pub const STRICT: AddressPolicy = AddressPolicy {
        allow_loopback: false,
    };
}

/// A URL that passed the guard and the addresses its connection must use.
#[derive(Debug, Clone)]
pub struct VettedTarget {
    pub url: Url,
    pub addrs: Vec<SocketAddr>,
}

fn blocked_v4(ip: Ipv4Addr, policy: AddressPolicy) -> Option<&'static str> {
    let [a, b, c, _] = ip.octets();
    if ip.is_loopback() {
        return if policy.allow_loopback {
            None
        } else {
            Some("loopback")
        };
    }
    if ip.is_link_local() {
        return Some("link-local (includes the cloud metadata endpoint)");
    }
    if a == 0 || a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168) {
        return Some("a private network");
    }
    if a == 100 && (64..=127).contains(&b) {
        return Some("shared address space (CGNAT / RFC 6598)");
    }
    if (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return Some("a reserved range");
    }
    if ip.is_multicast() {
        return Some("multicast");
    }
    if a >= 240 {
        return Some("a reserved range");
    }
    None
}

/// IPv4 addresses an IPv6 address carries: IPv4-mapped, IPv4-compatible, 6to4, Teredo (server and
/// client) and NAT64. A gateway that translates these forwards the connection to the embedded
/// address, so `[64:ff9b::7f00:1]` can reach 127.0.0.1 although it looks public.
fn embedded_v4(ip: Ipv6Addr) -> Vec<Ipv4Addr> {
    let seg = ip.segments();
    let oct = ip.octets();
    let mut found = Vec::new();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        found.push(mapped);
    }
    if seg[..6].iter().all(|s| *s == 0) {
        found.push(Ipv4Addr::new(oct[12], oct[13], oct[14], oct[15]));
    }
    if seg[0] == 0x2002 {
        found.push(Ipv4Addr::new(oct[2], oct[3], oct[4], oct[5]));
    }
    if seg[0] == 0x2001 && seg[1] == 0 {
        found.push(Ipv4Addr::new(oct[4], oct[5], oct[6], oct[7]));
        found.push(Ipv4Addr::new(!oct[12], !oct[13], !oct[14], !oct[15]));
    }
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6].iter().all(|s| *s == 0) {
        found.push(Ipv4Addr::new(oct[12], oct[13], oct[14], oct[15]));
    }
    found
}

/// Why `ip` may not be fetched, or `None` when it may.
pub fn blocked_reason(ip: IpAddr, policy: AddressPolicy) -> Option<String> {
    match ip {
        IpAddr::V4(v4) => blocked_v4(v4, policy).map(str::to_owned),
        IpAddr::V6(v6) => {
            // `::1` is the IPv4-compatible form of 0.0.0.1; judge it as loopback, not as v4.
            if v6.is_loopback() {
                return if policy.allow_loopback {
                    None
                } else {
                    Some("loopback".to_owned())
                };
            }
            for inner in embedded_v4(v6) {
                if let Some(reason) = blocked_v4(inner, policy) {
                    return Some(format!("{reason} (carried inside an IPv6 address)"));
                }
            }
            let first = v6.segments()[0];
            if v6.is_unspecified() {
                Some("a reserved range".to_owned())
            } else if first & 0xffc0 == 0xfe80 {
                Some("link-local".to_owned())
            } else if first & 0xfe00 == 0xfc00 {
                Some("a private network".to_owned())
            } else if first & 0xff00 == 0xff00 {
                Some("multicast".to_owned())
            } else if first == 0x2001 && v6.segments()[1] == 0x0db8 {
                Some("a reserved range".to_owned())
            } else {
                None
            }
        }
    }
}

/// Look at the authority exactly as written. Parsers disagree about where the host ends
/// (`http://127.0.0.1\@example.com/` is example.com to some and 127.0.0.1 to a browser), and
/// credentials are never needed to read a page.
fn authority_is_plain(raw: &str) -> bool {
    let Some(rest) = raw.split_once("://").map(|(_, rest)| rest) else {
        return false;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    !authority.contains('@') && !authority.contains('\\')
}

/// Vet `raw` and resolve it. Names are resolved once here; every answer must be allowed, so a name
/// with both a public and a private record cannot slip through, and the connection is later pinned
/// to these addresses.
pub fn vet(raw: &str, policy: AddressPolicy) -> Result<VettedTarget, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "url is not valid".to_owned())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("url must start with http:// or https://".to_owned());
    }
    if !authority_is_plain(raw.trim()) {
        return Err("url must not contain credentials or backslashes in the host part".to_owned());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "url has no port".to_owned())?;
    let addrs: Vec<SocketAddr> = match url.host() {
        None => return Err("url has no host".to_owned()),
        Some(Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(Host::Domain(name)) => (name, port)
            .to_socket_addrs()
            .map_err(|error| format!("could not resolve {name}: {error}"))?
            .collect(),
    };
    if addrs.is_empty() {
        return Err("host did not resolve to any address".to_owned());
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    for addr in &addrs {
        if let Some(reason) = blocked_reason(addr.ip(), policy) {
            return Err(format!(
                "refusing to fetch {host} ({}): {reason}",
                addr.ip()
            ));
        }
    }
    Ok(VettedTarget { url, addrs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(ip: &str) -> bool {
        blocked_reason(ip.parse().unwrap(), AddressPolicy::STRICT).is_some()
    }

    #[test]
    fn private_loopback_link_local_and_reserved_space_is_blocked() {
        for ip in [
            "127.0.0.1",
            "127.1.2.3",
            "10.0.0.1",
            "172.16.5.5",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "192.0.2.1",
            "198.18.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "2001:db8::1",
        ] {
            assert!(blocked(ip), "{ip} must be blocked");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "172.15.0.1",
            "172.32.0.1",
            "100.63.0.1",
            "100.128.0.1",
            "2606:4700:4700::1111",
        ] {
            assert!(!blocked(ip), "{ip} must be allowed");
        }
    }

    #[test]
    fn ipv4_carried_inside_ipv6_is_judged_as_the_ipv4_address() {
        for ip in [
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::",
            "2002:a9fe:a9fe::1",
            "::7f00:1",
        ] {
            assert!(blocked(ip), "{ip} must be blocked");
        }
        assert!(!blocked("::ffff:8.8.8.8"));
        assert!(!blocked("64:ff9b::808:808"));
    }

    #[test]
    fn only_loopback_is_relaxed_for_tests() {
        let relaxed = AddressPolicy {
            allow_loopback: true,
        };
        assert!(blocked_reason("127.0.0.1".parse().unwrap(), relaxed).is_none());
        assert!(blocked_reason("::1".parse().unwrap(), relaxed).is_none());
        assert!(blocked_reason("169.254.169.254".parse().unwrap(), relaxed).is_some());
        assert!(blocked_reason("10.0.0.1".parse().unwrap(), relaxed).is_some());
    }

    #[test]
    fn urls_with_credentials_backslashes_or_odd_schemes_are_refused() {
        for url in [
            "http://user:pass@8.8.8.8/",
            "http://127.0.0.1\\@example.com/",
            "http://8.8.8.8\\.example.com/",
            "ftp://8.8.8.8/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "not a url",
            "http:///nohost",
        ] {
            assert!(vet(url, AddressPolicy::STRICT).is_err(), "{url}");
        }
        // The same characters after the authority are ordinary path text.
        assert!(vet("http://8.8.8.8/a@b", AddressPolicy::STRICT).is_ok());
    }

    #[test]
    fn literal_private_hosts_are_refused_without_a_lookup() {
        for url in [
            "http://127.0.0.1/",
            "http://[::1]:8080/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.5:11434/",
            "http://[::ffff:7f00:1]/",
            "http://2130706433/",
            "http://0x7f.1/",
        ] {
            assert!(vet(url, AddressPolicy::STRICT).is_err(), "{url}");
        }
        let ok = vet("https://8.8.8.8/x", AddressPolicy::STRICT).unwrap();
        assert_eq!(ok.addrs, vec!["8.8.8.8:443".parse::<SocketAddr>().unwrap()]);
    }
}
