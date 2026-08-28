//! Answers the one question a failed request cannot: is the machine itself off
//! the network, or is the link up and Slack simply unreachable right now?
//!
//! Telegram makes the same distinction — "waiting for network" versus
//! "connecting" — and the two need different patience from the reader.
//!
//! Neither half of the answer costs a packet. That matters: this runs on the
//! shell's tick, so a socket that quietly dies under a closed lid is noticed in
//! seconds rather than whenever something next happens to make a request.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

/// How long the Slack reachability probe waits before calling the host dead.
const REACH_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether the machine has a link that could carry traffic off itself.
///
/// Two questions, because either one alone lies:
///
/// - A routing table with no default route is definitive, but a VPN tunnel
///   holds its own default route long after the Wi-Fi under it is switched off,
///   so a route on its own reads as healthy when nothing can move.
/// - A carrying interface says the hardware is associated, but says nothing
///   about whether anything is routed over it.
pub fn has_usable_link() -> bool {
    has_default_route() && has_carrying_interface()
}

/// Whether any default route exists.
///
/// `connect` on a UDP socket is a route lookup, not a packet: it fails with
/// `ENETUNREACH`/`EHOSTUNREACH` the moment the route goes away, and answers
/// instantly without putting anything on the wire. Both families are tried
/// because an IPv6-only link has no route to a v4 address and vice versa.
fn has_default_route() -> bool {
    routes_to(
        "0.0.0.0:0",
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53),
    ) || routes_to(
        "[::]:0",
        SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
            53,
        ),
    )
}

fn routes_to(bind: &str, target: SocketAddr) -> bool {
    UdpSocket::bind(bind).is_ok_and(|socket| socket.connect(target).is_ok())
}

/// Whether any real link is up and carrying an address of its own.
///
/// Wi-Fi switching off strips `en0` of its address and its running flag while
/// leaving every tunnel on the machine untouched, so this is what actually
/// changes at the moment the reader unplugs. Loopback and point-to-point
/// interfaces are skipped precisely because tunnels are point-to-point, and
/// link-local addresses do not count — `awdl0` and friends carry one at all
/// times and reach nothing.
#[cfg(unix)]
fn has_carrying_interface() -> bool {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `list` with an owned list on success, which is
    // handed straight back to `freeifaddrs` below. Every entry is read behind a
    // null check and nothing outlives the list.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            // Nothing to go on. Claiming the machine is offline on the strength
            // of a failed syscall would park a spinner on a working client.
            return true;
        }
        let mut carrying = false;
        let mut cursor = list;
        while !cursor.is_null() {
            let entry = &*cursor;
            cursor = entry.ifa_next;
            if carrying {
                continue;
            }
            const NEEDED: u32 = (libc::IFF_UP | libc::IFF_RUNNING) as u32;
            const SKIPPED: u32 = (libc::IFF_LOOPBACK | libc::IFF_POINTOPOINT) as u32;
            if entry.ifa_flags & NEEDED != NEEDED || entry.ifa_flags & SKIPPED != 0 {
                continue;
            }
            carrying = carries_address(entry.ifa_addr);
        }
        libc::freeifaddrs(list);
        carrying
    }
}

/// Whether this address is one that can reach off the machine.
///
/// # Safety
///
/// `addr` must be null or a valid `sockaddr` whose family tags the storage
/// behind it, as `getifaddrs` guarantees for the entries it owns.
#[cfg(unix)]
unsafe fn carries_address(addr: *const libc::sockaddr) -> bool {
    if addr.is_null() {
        return false;
    }
    unsafe {
        match (*addr).sa_family as libc::c_int {
            libc::AF_INET => {
                let inet = &*(addr as *const libc::sockaddr_in);
                let octets = u32::from_be(inet.sin_addr.s_addr).to_be_bytes();
                // 169.254/16 is a link that failed to get a lease.
                octets[..2] != [169, 254]
            }
            libc::AF_INET6 => {
                let inet6 = &*(addr as *const libc::sockaddr_in6);
                let octets = inet6.sin6_addr.s6_addr;
                // fe80::/10 link-local, fc00::/7 unique-local: neither routes.
                octets[0] != 0xfe && octets[0] & 0xfe != 0xfc
            }
            _ => false,
        }
    }
}

#[cfg(not(unix))]
fn has_carrying_interface() -> bool {
    true
}

/// Whether `host` accepts a TLS-port connection right now.
///
/// Blocking on purpose — DNS resolution is — so callers hand it to a blocking
/// task. Nothing is sent: the handshake never starts, so this costs Slack a
/// dropped SYN and tells us the link carries traffic again.
pub fn is_reachable(host: &str) -> bool {
    let Ok(addresses) = (host, 443).to_socket_addrs() else {
        return false;
    };
    addresses.into_iter().any(|address| {
        TcpStream::connect_timeout(&address, REACH_TIMEOUT)
            .inspect(|stream| {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            })
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe has to answer without panicking or blocking on a box with no
    /// network at all, which is exactly where it gets asked.
    #[test]
    fn link_probe_answers() {
        let _ = has_usable_link();
    }

    /// A test box is on a network, and a machine that can run this suite must
    /// never be told it has none — a false "waiting for network" parks a
    /// spinner on a perfectly healthy client.
    #[test]
    fn a_networked_machine_is_never_called_offline() {
        if !has_default_route() {
            return;
        }
        assert!(
            has_carrying_interface(),
            "a routable machine reported no carrying interface"
        );
    }

    #[test]
    fn unresolvable_host_is_unreachable() {
        assert!(!is_reachable("host.invalid."));
    }
}
