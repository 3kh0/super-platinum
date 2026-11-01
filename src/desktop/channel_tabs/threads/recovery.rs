//! A NEW outage invocation can acquire current positive authority for the same
//! retained selection. Retired UI callbacks/Gates never renew themselves.
use super::*;
use crate::bootstrap::reconnect::Visit as Restore;
enum Drive {
    Wait(tokio::sync::watch::Receiver<bool>),
    Read(Source),
}
fn same_cache(old: &Route, next: &Route, s: &ShellState) -> bool {
    old.tab == next.tab
        && old.gate.account == next.gate.account
        && old.gate.team == next.gate.team
        && old.gate.channel == next.gate.channel
        && old.gate.owner.current(&s.core)
        && s.core.client.same_configuration(&old.gate.client)
}
pub(crate) async fn after_outage(mut state: Signal<ShellState>, restore: Restore) {
    let drive = {
        let mut s = state.write();
        if !restore.current(&s)
            || s.overlay.is_some()
            || !s.conversation_visible()
            || s.channel_tab != crate::state::ChannelTab::Threads
        {
            return;
        }
        let Some(slot) = super::super::selection::retained_thread(&s) else {
            return;
        };
        if let Some(done) = read::coalesced(&s) {
            Drive::Wait(done)
        } else {
            // No await and no stale authority in this acquisition. Missing native
            // canonical/assignment/settled-boot facts refuse, never default ready.
            let route = if slot.route.current(&s) {
                slot.route.clone()
            } else {
                let Some(base) = super::super::inventory(&s) else {
                    return;
                };
                let Some(next) = Route::capture(&s, &base, &slot.route.tab) else {
                    return;
                };
                next
            };
            let Some(native) = Visit::capture(&s) else {
                return;
            };
            if !native.native_read_current(&s) || !route.current(&s) {
                return;
            }
            if route != slot.route
                && !super::super::selection::renew_thread(&mut s, &slot, route.clone())
            {
                return;
            }
            let old = &s.channel_tabs.threads;
            if old.route.as_ref() != Some(&route) || old.pending.is_some() {
                let keep = old
                    .route
                    .as_ref()
                    .is_some_and(|r| same_cache(r, &route, &s));
                let mut old = std::mem::take(&mut s.channel_tabs.threads);
                s.channel_tabs.threads = State {
                    route: Some(route),
                    sequence: old.sequence,
                    accepted: if keep { old.accepted.take() } else { None },
                    local: if keep { old.local.take() } else { None },
                    ..Default::default()
                };
            }
            let Some(source) = Source::capture(&s) else {
                return;
            };
            Drive::Read(source)
        }
    };
    match drive {
        Drive::Read(source) => read::run_owned(state, source, Some(restore)).await,
        Drive::Wait(mut done) => {
            // The outer restore must not race its open-thread refresh against
            // this original read's canonical publication. No polling/new HTTP.
            let wait = async {
                loop {
                    if *done.borrow() {
                        break;
                    }
                    if done.changed().await.is_err() {
                        break;
                    }
                }
            };
            wait.await;
        }
    }
}
