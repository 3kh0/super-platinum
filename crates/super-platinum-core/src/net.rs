//! Answers the one question a failed request cannot: is the machine itself off
//! the network, or is the link up and Slack simply unreachable right now?
//!
//! Telegram makes the same distinction — "waiting for network" versus
//! "connecting" — and the two need different patience from the reader.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

/// How long the Slack reachability probe waits before calling the host dead.
const REACH_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether the machine has any route off itself.
///
/// `connect` on a UDP socket is a route lookup, not a packet: it fails with
/// `ENETUNREACH`/`EHOSTUNREACH` the moment the interface goes away, and answers
/// instantly without putting anything on the wire. Both families are tried
/// because an IPv6-only link has no route to a v4 address and vice versa.
pub fn has_network_route() -> bool {
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
    fn route_probe_answers() {
        let _ = has_network_route();
    }

    #[test]
    fn unresolvable_host_is_unreachable() {
        assert!(!is_reachable("host.invalid."));
    }
}
