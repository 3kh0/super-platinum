//! Explicit navigation only to an already accepted native canonical joined row.
//! A metadata result, public vector or false preview flag grants nothing here.
use super::*;
use crate::app_home::channel_fetches::canonical::{self, Canonical};
use std::future::Future;

#[derive(Clone)]
pub(super) struct Action {
    page: Arc<Loaded>,
    canonical: Arc<Canonical>,
    post_boot: Arc<crate::app_home::cfm_lifecycle::PostBoot>,
    revision: u64,
    selection: Arc<()>,
    channel: String,
}
fn ordinary_member(canonical: &Canonical, id: &str) -> bool {
    let Some(row) = canonical.row(id) else {
        return false;
    };
    // The existing ordinary-label classifier excludes DM/file/record/huddle/
    // meeting/unknown shapes; it is only a shape check, not an authority mint.
    row["is_member"] == true
        && row
            .get("is_archived")
            .is_none_or(|v| v.is_null() || v == false)
        && (row["is_channel"] == true || row["is_private"] == true || row["is_group"] == true)
        && canonical::ordinary_label(&serde_json::json!({"type":"channel","channel_id":id}), row)
            .is_ok()
}
pub(super) fn capture(
    s: &ShellState,
    page: Arc<Loaded>,
    revision: u64,
    id: &str,
) -> Option<Action> {
    if s.core.screen != super_platinum_core::state::Screen::Main
        || !page.current(s)
        || s.channel_directory.busy()
        || s.channel_directory.revision != revision
        || s.channel_directory.selected.as_deref() != Some(id)
        || page.query.scope != Scope::Joined
        || !page
            .page
            .items
            .iter()
            .any(|i| i.id == id && i.is_member == Some(true))
    {
        return None;
    }
    let team = &page.entry.family.session.team_id;
    let canonical = canonical::observed(s, team).ok()?;
    let post_boot = Arc::new(crate::app_home::cfm_lifecycle::post_boot(s, team).ok()?);
    if !post_boot.current(s)
        || !ordinary_member(&canonical, id)
        || !s.core.workspaces.get(team)?.channels.contains_key(id)
        || !s.channels.iter().any(|c| c.id == id)
    {
        return None;
    }
    Some(Action {
        page,
        canonical,
        post_boot,
        revision,
        selection: s.channel_directory.selected_origin.clone(),
        channel: id.into(),
    })
}
impl Action {
    fn current(&self, s: &ShellState) -> bool {
        s.core.screen == super_platinum_core::state::Screen::Main
            && self.page.current(s)
            && !s.channel_directory.busy()
            && s.channel_directory.revision == self.revision
            && Arc::ptr_eq(&s.channel_directory.selected_origin, &self.selection)
            && self.post_boot.current(s)
            && s.channel_directory.selected.as_deref() == Some(self.channel.as_str())
            && self
                .canonical
                .current_for(s, &self.page.entry.family.session.team_id)
            && ordinary_member(&self.canonical, &self.channel)
    }
    pub(super) fn apply(&self, mut state: Signal<ShellState>) -> Option<Work> {
        let mut s = state.try_write().ok()?;
        if !self.current(&s) {
            return None;
        }
        let index = s.channels.iter().position(|c| c.id == self.channel)?;
        // Stable identity is resolved again; a reordered list cannot redirect it.
        s.dismiss_overlay();
        s.select_channel(index, crate::state::ChannelOpen::Global);
        let frame = Frame::capture(&s);
        let reconnect = s.reconnect_generation;
        drop(s);
        Some(Work {
            family: self.page.entry.family.clone(),
            canonical: self.canonical.clone(),
            post_boot: self.post_boot.clone(),
            frame,
            reconnect,
            realtime: self.page.realtime,
            channels: self.page.channels,
            channel: self.channel.clone(),
        })
    }
    pub(super) fn start(&self, state: Signal<ShellState>) {
        if let Some(work) = self.apply(state) {
            dioxus::core::spawn_forever(
                work.run_with(state, crate::bootstrap::refresh_selected_channel),
            );
        }
    }
}
pub(super) struct Work {
    family: Arc<Family>,
    canonical: Arc<Canonical>,
    post_boot: Arc<crate::app_home::cfm_lifecycle::PostBoot>,
    frame: Frame,
    reconnect: u64,
    realtime: u64,
    channels: u64,
    channel: String,
}
impl Work {
    fn current(&self, s: &ShellState) -> bool {
        s.core.screen == super_platinum_core::state::Screen::Main
            && s.signed_in
            && !s.loading
            && s.overlay.is_none()
            && std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none()
            && self.family.current(s)
            && self.post_boot.current(s)
            && self.frame.current(s)
            && s.reconnect_generation == self.reconnect
            && s.core.active_team.as_ref() == Some(&self.family.session.team_id)
            && s.core.active_channel.as_deref() == Some(self.channel.as_str())
            && self.canonical.current_for(s, &self.family.session.team_id)
            && ordinary_member(&self.canonical, &self.channel)
            && s.core
                .workspaces
                .get(&self.family.session.team_id)
                .is_some_and(|w| {
                    w.rt_generation == self.realtime
                        && w.channel_lifecycle.revision() == self.channels
                })
    }
    pub(super) async fn run_with<F, Fut>(self, state: Signal<ShellState>, refresh: F)
    where
        F: FnOnce(Signal<ShellState>) -> Fut,
        Fut: Future<Output = ()>,
    {
        // Spend no read/mark/navigation-history work under a replacement visit.
        // The existing history loader owns its unchanged per-await guards after
        // this original-owner fence. Release the signal borrow before polling it.
        let current = state.try_peek().is_ok_and(|s| self.current(&s));
        if current {
            refresh(state).await;
        }
    }
}
