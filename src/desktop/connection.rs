//! The rail's connection indicator.
//!
//! Two things have to agree before the shell calls itself connected: the
//! realtime socket is up, and the last Slack request actually reached Slack.
//! A lid closing takes the second one down long before the first notices, which
//! is exactly the case this exists for — the reader sees a spinner instead of a
//! transport error carrying a signed URL.

use std::time::{Duration, Instant};

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::net;
use super_platinum_core::slack::transport::Health;

use crate::state::{ConnectionStatus, ConnectionVm, ShellState};

/// How long a drop has to last before the rail says anything. A socket that
/// comes back inside this window is not news worth blinking a spinner over.
const GRACE: Duration = Duration::from_millis(1_500);

/// How often the machine's own routing table is consulted. This is the fast
/// path: Wi-Fi switching off is not a request failure, and waiting for one to
/// time out is how the indicator used to miss the whole event.
const ROUTE_EVERY: Duration = Duration::from_secs(2);

/// How often the shell re-asks whether Slack answers, while down but routable.
const PROBE_EVERY: Duration = Duration::from_secs(3);

/// How long a "connecting" stretch has to last before coming back counts as an
/// outage worth reloading after. A cold boot spends a few seconds here while the
/// socket comes up, and re-fetching what boot just fetched is pure waste.
const RELOAD_AFTER: Duration = Duration::from_secs(20);

/// Where reachability is measured when no workspace names a host of its own.
/// Slack's own front door, not a captive-portal checker: "the network is up but
/// Slack is not" is still a spinner here.
const FALLBACK_HOST: &str = "slack.com";

/// What the tick should do once the indicator has been folded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Next {
    #[default]
    Nothing,
    /// Ask whether Slack answers. The answer is a blocking syscall, so it does
    /// not belong on the tick — and it is what releases held requests.
    Probe,
    /// The link came back after a real outage. Whatever the reader is looking
    /// at was left stale by loads that failed while it was down, and nothing
    /// else will ever ask for it again.
    Reload,
}

/// Folds realtime status and transport health into what the rail paints.
pub fn evaluate(shell: &mut ShellState, now: Instant) -> Next {
    // The loading screen and the signed-out shell have no link to describe, and
    // neither does a window with no transport behind it. Whatever the indicator
    // says in those states was put there on purpose.
    let Some(health) = shell
        .core
        .transport
        .as_ref()
        .map(|transport| transport.health())
    else {
        return Next::Nothing;
    };
    if !shell.signed_in || shell.loading {
        return Next::Nothing;
    }

    // Two kernel lookups — the routing table and the interface list — with no
    // packet, no DNS, and nothing to block on, so the tick can afford to ask
    // outright instead of waiting for a request to fail. Waiting was the bug:
    // an established socket over a link that goes away does not error, it goes
    // quiet, and a request over it hangs instead of failing.
    if shell
        .connection
        .routed_at
        .is_none_or(|at| now.duration_since(at) >= ROUTE_EVERY)
    {
        shell.connection.routable = net::has_usable_link();
        shell.connection.routed_at = Some(now);
    }

    // Telling the transport the link is gone is what holds requests back: a
    // call fired into a dead link does not fail, it hangs, and the pane behind
    // it sits on "Loading…" until something times out.
    if !shell.connection.routable
        && health != Health::Offline
        && let Some(transport) = shell.core.transport.as_ref()
    {
        transport.set_health(Health::Offline);
    }

    let live = shell.connection.routable && health != Health::Offline && realtime_live(shell);
    if live {
        let outage = shell.connection.status == ConnectionStatus::NoNetwork
            || shell
                .connection
                .unstable_since
                .is_some_and(|since| now.duration_since(since) >= RELOAD_AFTER);
        let routable = shell.connection.routable;
        let routed_at = shell.connection.routed_at;
        shell.connection = ConnectionVm {
            routable,
            routed_at,
            ..ConnectionVm::default()
        };
        return if outage { Next::Reload } else { Next::Nothing };
    }

    let since = *shell.connection.unstable_since.get_or_insert(now);
    if now.duration_since(since) < GRACE {
        return Next::Nothing;
    }
    shell.connection.status = if shell.connection.routable {
        ConnectionStatus::Connecting
    } else {
        ConnectionStatus::NoNetwork
    };
    // Probed even when the link looks dead: this is the only path that can
    // correct a link check that read an exotic setup wrong, and with no network
    // behind it the connect fails fast anyway.
    let due = shell
        .connection
        .probed_at
        .is_none_or(|at| now.duration_since(at) >= PROBE_EVERY);
    if shell.connection.probing || !due {
        return Next::Nothing;
    }
    shell.connection.probing = true;
    Next::Probe
}

/// Asks whether Slack answers right now.
///
/// A successful reach clears the transport's offline mark: that mark is only
/// ever set by a failed request, so without this the rail would keep saying
/// "connecting" until something else happened to call Slack.
pub async fn probe(mut state: Signal<ShellState>) {
    let host = active_host(&state);
    let reachable = tokio::task::spawn_blocking(move || net::is_reachable(&host))
        .await
        .unwrap_or(false);

    let mut shell = state.write();
    shell.connection.probing = false;
    shell.connection.probed_at = Some(Instant::now());
    if reachable && let Some(transport) = shell.core.transport.as_ref() {
        // "Unknown", not "Online": the link carries traffic again, but Slack's
        // API has not answered anything yet. That is the realtime socket's
        // question to settle.
        transport.set_health(Health::Unknown);
    }
}

/// The host held requests are waiting on: this workspace's own, since an
/// enterprise grid answers somewhere other than `slack.com`.
fn active_host(state: &Signal<ShellState>) -> String {
    let shell = state.read();
    shell
        .core
        .session
        .as_ref()
        .zip(shell.core.active_team.as_ref())
        .and_then(|(session, team)| session.workspaces.get(team))
        .map(super_platinum_core::slack::api_host)
        .unwrap_or_else(|| FALLBACK_HOST.to_owned())
}

fn realtime_live(shell: &ShellState) -> bool {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .is_some_and(|workspace| workspace.rt.is_connected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;
    use crate::state::ConnectionStatus;
    use super_platinum_core::slack::Transport;
    use super_platinum_core::slack::realtime::Connection;
    use super_platinum_core::state::RealtimeStatus;

    /// A signed-in shell with a live socket and a route, with the route already
    /// answered so the tests never touch the machine's real network.
    fn live_shell(now: Instant) -> ShellState {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.signed_in = true;
        shell.loading = false;
        shell.core.transport = Some(std::sync::Arc::new(
            Transport::new("cookie").expect("transport"),
        ));
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        shell
            .core
            .workspaces
            .get_mut("T1")
            .expect("fixture workspace")
            .rt = RealtimeStatus::Connected(Connection::from_sender(tx));
        shell.connection.routable = true;
        shell.connection.routed_at = Some(now);
        shell
    }

    fn set_health(shell: &ShellState, health: Health) {
        shell
            .core
            .transport
            .as_ref()
            .expect("transport")
            .set_health(health);
    }

    #[test]
    fn a_healthy_shell_says_nothing_and_probes_nothing() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        assert_eq!(evaluate(&mut shell, now), Next::Nothing);
        assert_eq!(shell.connection.indicator(), None);
    }

    /// The reported bug: Wi-Fi switched off, every socket still believing it is
    /// connected because nothing has tried to use it yet. The routing table is
    /// the only thing that knows, so the routing table is what the tick asks.
    #[test]
    fn a_lost_route_lights_the_rail_without_waiting_for_a_request_to_fail() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        shell.connection.routable = false;

        evaluate(&mut shell, now);
        assert_eq!(
            shell.connection.indicator(),
            None,
            "inside the grace window"
        );
        // A probe is still asked for: a link check that read the machine wrong
        // has to have something that can correct it.
        assert_eq!(evaluate(&mut shell, now + GRACE), Next::Probe);
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::NoNetwork)
        );
    }

    #[test]
    fn a_blink_of_a_drop_never_reaches_the_rail() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        set_health(&shell, Health::Offline);

        evaluate(&mut shell, now);
        assert_eq!(
            shell.connection.indicator(),
            None,
            "inside the grace window"
        );

        // Back before the grace runs out: the reader is never told.
        set_health(&shell, Health::Online);
        shell.connection.routed_at = Some(now + Duration::from_millis(400));
        assert_eq!(
            evaluate(&mut shell, now + Duration::from_millis(400)),
            Next::Nothing,
            "a blink is not an outage worth reloading after"
        );
        assert_eq!(shell.connection.indicator(), None);
        assert_eq!(shell.connection.unstable_since, None);
    }

    #[test]
    fn a_lasting_drop_paints_the_indicator_and_asks_for_a_probe() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        set_health(&shell, Health::Offline);

        evaluate(&mut shell, now);
        shell.connection.routed_at = Some(now + GRACE);
        assert_eq!(evaluate(&mut shell, now + GRACE), Next::Probe);
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::Connecting)
        );

        // One probe at a time: the tick fires five times a second.
        let later = now + GRACE + Duration::from_millis(200);
        shell.connection.routed_at = Some(later);
        assert_eq!(evaluate(&mut shell, later), Next::Nothing);
    }

    /// Coming back from a real outage has to re-drive the surface: every load
    /// that fired while the link was down failed, and nothing else asks again.
    #[test]
    fn coming_back_from_an_outage_reloads_the_surface() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        shell.connection.routable = false;
        evaluate(&mut shell, now);
        evaluate(&mut shell, now + GRACE);
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::NoNetwork)
        );

        // What the probe does once Slack answers again: the link check alone
        // never releases held requests, confirmation does.
        let back = now + GRACE + Duration::from_secs(1);
        set_health(&shell, Health::Unknown);
        shell.connection.routable = true;
        shell.connection.routed_at = Some(back);
        assert_eq!(evaluate(&mut shell, back), Next::Reload);
        assert_eq!(shell.connection.indicator(), None);

        // And only on the transition — a healthy tick asks for nothing.
        shell.connection.routed_at = Some(back);
        assert_eq!(evaluate(&mut shell, back), Next::Nothing);
    }

    /// A cold boot spends a few seconds without a socket. Reloading there would
    /// re-fetch everything boot just fetched.
    #[test]
    fn a_boot_handshake_is_not_treated_as_an_outage() {
        let now = Instant::now();
        let mut shell = live_shell(now);
        shell
            .core
            .workspaces
            .get_mut("T1")
            .expect("fixture workspace")
            .rt = RealtimeStatus::Disconnected;
        evaluate(&mut shell, now);
        let landed = now + GRACE + Duration::from_secs(2);
        shell.connection.routed_at = Some(landed);
        evaluate(&mut shell, landed);

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        shell
            .core
            .workspaces
            .get_mut("T1")
            .expect("fixture workspace")
            .rt = RealtimeStatus::Connected(Connection::from_sender(tx));
        assert_eq!(evaluate(&mut shell, landed), Next::Nothing);
    }

    /// A window with no session behind it — a fixture capture — owns whatever
    /// the indicator says, and the tick must not scrub it.
    #[test]
    fn a_fixture_keeps_the_indicator_it_was_given() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.connection.status = ConnectionStatus::NoNetwork;
        assert_eq!(evaluate(&mut shell, Instant::now()), Next::Nothing);
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::NoNetwork)
        );
    }
}
