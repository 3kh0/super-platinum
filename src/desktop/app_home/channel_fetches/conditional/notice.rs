//! Per-team leading/trailing source stale-notification throttle.
use super::*;
#[derive(Default)]
pub(super) struct State {
    deadline: Option<Instant>,
    trailing: bool,
    timer: Option<Arc<()>>,
}
impl State {
    pub(super) fn invoke(&mut self, now: Instant) -> bool {
        if self.deadline.is_none() {
            self.deadline =
                Some(now + Duration::from_millis(freshness::STALE_NOTIFICATION_THROTTLE_MS));
            true
        } else {
            self.trailing = true;
            false
        }
    }
}
pub(super) fn schedule(mut state: Signal<ShellState>) {
    let timers = {
        let mut s = state.write();
        s.app_home
            .channel_fetches
            .conditional
            .slots
            .iter_mut()
            .filter_map(|(key, r)| {
                let deadline = r.notice.deadline?;
                if r.notice.timer.is_some() {
                    return None;
                }
                let token = Arc::new(());
                r.notice.timer = Some(token.clone());
                Some((
                    key.clone(),
                    r.context.clone(),
                    r.identity.clone(),
                    token,
                    deadline,
                ))
            })
            .collect::<Vec<_>>()
    };
    for (key, ctx, identity, token, deadline) in timers {
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep_until(deadline.into()).await;
            let mut s = state.write();
            let current = ctx.account_current(&s);
            let Some(r) = s.app_home.channel_fetches.conditional.slots.get_mut(&key) else {
                return;
            };
            if !Arc::ptr_eq(&r.identity, &identity)
                || !r
                    .notice
                    .timer
                    .as_ref()
                    .is_some_and(|t| Arc::ptr_eq(t, &token))
            {
                return;
            }
            r.notice.timer = None;
            r.notice.deadline = None;
            let notify = std::mem::take(&mut r.notice.trailing) && current;
            if notify && let Some(slot) = s.app_home.channel_fetches.slots.get(&key) {
                slot.notify();
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_notifications_coalesce_but_keep_one_trailing_invocation() {
        let mut s = State::default();
        let now = Instant::now();
        assert!(s.invoke(now));
        assert!(!s.invoke(now + Duration::from_millis(20)));
        assert!(!s.invoke(now + Duration::from_millis(50)));
        assert!(s.trailing);
        assert_eq!(s.deadline, Some(now + Duration::from_secs(2)));
    }
}
