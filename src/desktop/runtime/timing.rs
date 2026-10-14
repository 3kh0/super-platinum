//! Current-time housekeeping must not replay expired ticks after suspension.
use std::time::{Duration, Instant};
use super_platinum_core::state::Workspace;

const TICK: Duration = Duration::from_millis(200);
const MEDIA_SWEEP: Duration = Duration::from_secs(1);

pub(super) fn interval() -> tokio::time::Interval {
    let mut interval = tokio::time::interval(TICK);
    // Every consumer evaluates its current deadline; missed polls are not work
    // that should run repeatedly before the event loop can respond again.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}

/// Render-registered media needs a sweep even without a following Slack call.
/// Admit the first tick immediately and one check after an elapsed second. A
/// long pause admits only one check, then starts a new one-second deadline.
/// Pending/loading/offline checks remain with the caller and still consume this
/// cadence, so an ineligible sweep cannot turn into a five-Hz fetch loop.
#[derive(Default)]
pub(super) struct MediaSweep {
    checked_at: Option<Instant>,
}
impl MediaSweep {
    pub(super) fn due(&mut self, now: Instant) -> bool {
        if self
            .checked_at
            .is_some_and(|at| now.saturating_duration_since(at) < MEDIA_SWEEP)
        {
            return false;
        }
        self.checked_at = Some(now);
        true
    }
}

pub(super) fn prune_typing<'a>(
    workspaces: impl Iterator<Item = &'a mut Workspace>,
    now: Instant,
) -> bool {
    let mut changed = false;
    for workspace in workspaces {
        // Do not short-circuit after the first changed workspace: one resumed
        // tick must expire all stale indicators, preserving fresh ones.
        changed |= workspace.prune_typing(now, Duration::from_secs(4));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumed_interval_skips_expired_polls_instead_of_bursting() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                // An already elapsed timer models a stalled consumer without
                // sleeping the test or requiring Tokio's test-util feature.
                let expired = tokio::time::Instant::now() - Duration::from_secs(30);
                let mut burst = tokio::time::interval(TICK);
                burst.reset_at(expired);
                let first = burst.tick().await;
                let second = burst.tick().await;
                assert_eq!(second.duration_since(first), TICK);
                assert!(second < tokio::time::Instant::now());

                let mut skip = interval();
                skip.reset_at(expired);
                let resumed = tokio::time::Instant::now();
                assert_eq!(skip.tick().await, expired);
                let next = skip.tick().await;
                assert!(next > resumed, "next deadline must be in the future");
                assert!(next.duration_since(expired) > Duration::from_secs(29));
                assert_eq!(
                    next.duration_since(expired).as_nanos() % TICK.as_nanos(),
                    0,
                    "Skip retains the original interval phase"
                );
            });
    }

    #[test]
    fn media_sweep_is_immediate_then_elapsed_and_never_catches_up() {
        let start = Instant::now();
        let mut sweep = MediaSweep::default();
        assert!(sweep.due(start));
        for millis in [0, 200, 400, 600, 800, 999] {
            assert!(!sweep.due(start + Duration::from_millis(millis)));
        }
        assert!(sweep.due(start + MEDIA_SWEEP));
        let resumed = start + Duration::from_secs(600);
        assert!(sweep.due(resumed));
        for millis in [0, 200, 400, 600, 800, 999] {
            assert!(!sweep.due(resumed + Duration::from_millis(millis)));
        }
        assert!(sweep.due(resumed + MEDIA_SWEEP));
    }

    #[test]
    fn one_poll_expires_typing_in_every_workspace_and_keeps_fresh_entries() {
        let now = Instant::now();
        let mut workspaces: Vec<_> = ["T1", "T2", "T3"]
            .into_iter()
            .map(|team| {
                let mut workspace =
                    Workspace::from_session(&super_platinum_core::config::WorkspaceSession {
                        team_id: team.into(),
                        user_id: "USELF".into(),
                        enterprise_id: None,
                        name: "Fixture".into(),
                        url: "https://example.test".into(),
                        token: "fixture".into(),
                    });
                workspace.set_typing("C1", "UOLD".into(), now - Duration::from_secs(5));
                workspace.set_typing("C1", "UFRESH".into(), now - Duration::from_secs(1));
                workspace
            })
            .collect();
        assert!(prune_typing(workspaces.iter_mut(), now));
        for workspace in &workspaces {
            assert_eq!(workspace.typing["C1"].len(), 1);
            assert_eq!(workspace.typing["C1"][0].0, "UFRESH");
        }
        assert!(!prune_typing(workspaces.iter_mut(), now));
    }
}
