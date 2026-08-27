//! The rail's connection indicator.
//!
//! Two things have to agree before the shell calls itself connected: the
//! realtime socket is up, and the last Slack request actually reached Slack.
//! A lid closing takes the second one down long before the first notices, which
//! is exactly the case this exists for — the reader sees a spinner instead of a
//! transport error carrying a signed URL.

use std::time::{Duration, Instant};

use dioxus::prelude::{Signal, WritableExt};
use super_platinum_core::net;
use super_platinum_core::slack::transport::Health;

use crate::state::{ConnectionStatus, ConnectionVm, ShellState};

/// How long a drop has to last before the rail says anything. A socket that
/// comes back inside this window is not news worth blinking a spinner over.
const GRACE: Duration = Duration::from_millis(1_500);

/// How often the shell re-asks the machine about its own network while down.
const PROBE_EVERY: Duration = Duration::from_secs(3);

/// Where reachability is measured. Slack's own front door, not a captive-portal
/// checker: "the network is up but Slack is not" is still a spinner here.
const REACH_HOST: &str = "slack.com";

/// Folds realtime status and transport health into what the rail paints.
///
/// Returns true when the caller should spawn a fresh [`probe`] — the answer to
/// "connecting, or no network at all?" is a blocking syscall and does not
/// belong on the tick.
pub fn evaluate(shell: &mut ShellState, now: Instant) -> bool {
    // The loading screen and the signed-out shell have no link to describe, and
    // neither does a window with no transport behind it. Whatever the indicator
    // says in those states was put there on purpose.
    let Some(health) = shell
        .core
        .transport
        .as_ref()
        .map(|transport| transport.health())
    else {
        return false;
    };
    if !shell.signed_in || shell.loading {
        return false;
    }
    if health != Health::Offline && realtime_live(shell) {
        shell.connection = ConnectionVm::default();
        return false;
    }

    let since = *shell.connection.unstable_since.get_or_insert(now);
    if now.duration_since(since) < GRACE {
        return false;
    }
    shell.connection.status = if shell.connection.routable {
        ConnectionStatus::Connecting
    } else {
        ConnectionStatus::NoNetwork
    };
    let due = shell
        .connection
        .probed_at
        .is_none_or(|at| now.duration_since(at) >= PROBE_EVERY);
    if shell.connection.probing || !due {
        return false;
    }
    shell.connection.probing = true;
    true
}

/// Asks the machine whether it has a network at all, and whether Slack answers.
///
/// A successful reach clears the transport's offline mark: that mark is only
/// ever set by a failed request, so without this the rail would keep saying
/// "connecting" until something else happened to call Slack.
pub async fn probe(mut state: Signal<ShellState>) {
    let answer = tokio::task::spawn_blocking(|| {
        let routable = net::has_network_route();
        (routable, routable && net::is_reachable(REACH_HOST))
    })
    .await;
    let (routable, reachable) = answer.unwrap_or((false, false));

    let mut shell = state.write();
    shell.connection.probing = false;
    shell.connection.probed_at = Some(Instant::now());
    shell.connection.routable = routable;
    if reachable && let Some(transport) = shell.core.transport.as_ref() {
        // "Unknown", not "Online": the link carries traffic again, but Slack's
        // API has not answered anything yet. That is the realtime socket's
        // question to settle.
        transport.set_health(Health::Unknown);
    }
    if shell.connection.status != ConnectionStatus::Online {
        shell.connection.status = if routable {
            ConnectionStatus::Connecting
        } else {
            ConnectionStatus::NoNetwork
        };
    }
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

    fn live_shell() -> ShellState {
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
        shell
    }

    #[test]
    fn a_healthy_shell_says_nothing_and_probes_nothing() {
        let mut shell = live_shell();
        assert!(!evaluate(&mut shell, Instant::now()));
        assert_eq!(shell.connection.indicator(), None);
    }

    #[test]
    fn a_blink_of_a_drop_never_reaches_the_rail() {
        let mut shell = live_shell();
        let start = Instant::now();
        shell
            .core
            .transport
            .as_ref()
            .expect("transport")
            .set_health(Health::Offline);

        evaluate(&mut shell, start);
        assert_eq!(
            shell.connection.indicator(),
            None,
            "inside the grace window"
        );

        // Back before the grace runs out: the reader is never told.
        shell
            .core
            .transport
            .as_ref()
            .expect("transport")
            .set_health(Health::Online);
        assert!(!evaluate(&mut shell, start + Duration::from_millis(400)));
        assert_eq!(shell.connection.indicator(), None);
        assert_eq!(shell.connection.unstable_since, None);
    }

    #[test]
    fn a_lasting_drop_paints_the_indicator_and_asks_for_a_probe() {
        let mut shell = live_shell();
        let start = Instant::now();
        shell
            .core
            .transport
            .as_ref()
            .expect("transport")
            .set_health(Health::Offline);

        evaluate(&mut shell, start);
        assert!(evaluate(&mut shell, start + GRACE));
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::Connecting)
        );
        // One probe at a time: the tick fires five times a second.
        assert!(!evaluate(
            &mut shell,
            start + GRACE + Duration::from_millis(200)
        ));

        // No route off the machine at all is a different sentence.
        shell.connection.routable = false;
        evaluate(&mut shell, start + GRACE + Duration::from_millis(400));
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::NoNetwork)
        );
    }

    /// A window with no session behind it — a fixture capture — owns whatever
    /// the indicator says, and the tick must not scrub it.
    #[test]
    fn a_fixture_keeps_the_indicator_it_was_given() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.connection.status = ConnectionStatus::NoNetwork;
        assert!(!evaluate(&mut shell, Instant::now()));
        assert_eq!(
            shell.connection.indicator(),
            Some(ConnectionStatus::NoNetwork)
        );
    }
}
