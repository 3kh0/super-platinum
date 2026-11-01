//! Read-only native target observation and affine internal-navigation handoff.
//! Neither preparation nor canonical presence is the timestamp invocation.
use crate::{
    app_home::{
        cfm_lifecycle,
        channel_fetches::canonical::{self, Canonical},
    },
    bootstrap::ReadOwner,
    state::{ChannelTab, ChannelVm, MainView, ShellState},
};
use dioxus::prelude::{ReadableExt, Signal};
use serde_json::Value;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use super_platinum_core::{domain::PendingScrollTarget, slack::SlackClient, state::Screen};

fn id(value: &str, prefix: u8) -> bool {
    (9..=64).contains(&value.len())
        && value.as_bytes()[0] == prefix
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn timestamp(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 17
        && b[0] != b'0'
        && b[10] == b'.'
        && b[..10].iter().all(u8::is_ascii_digit)
        && b[11..].iter().all(u8::is_ascii_digit)
}
fn available(s: &ShellState) -> bool {
    s.core.screen == Screen::Main
        && s.signed_in
        && !s.loading
        && s.overlay.is_none()
        && s.app_modal.is_none()
        && std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none()
}

// Principal/canonical data, deliberately independent of the outgoing or incoming
// rail visit. No directory page/open token, mutable capture or native Release.
struct Target {
    owner: Arc<ReadOwner>,
    client: SlackClient,
    canonical: Arc<Canonical>,
    post_boot: Arc<cfm_lifecycle::PostBoot>,
    team: String,
    channel: String,
    ts: String,
    realtime: u64,
    lifecycle: u64,
    native_channel: Value,
    projected: ChannelVm,
}
impl Target {
    fn current(&self, s: &ShellState) -> bool {
        if !available(s)
            || s.core.active_team.as_deref() != Some(self.team.as_str())
            || !self.owner.current(&s.core)
            || !s.core.client.same_configuration(&self.client)
            || !self.canonical.current_for(s, &self.team)
            || !self.post_boot.current(s)
        {
            return false;
        }
        let Some(workspace) = s.core.workspaces.get(&self.team) else {
            return false;
        };
        let Some(channel) = workspace.channels.get(&self.channel) else {
            return false;
        };
        workspace.rt_generation == self.realtime
            && workspace.channel_lifecycle.revision() == self.lifecycle
            && ordinary(&self.canonical, &self.channel)
            && serde_json::to_value(channel).ok().as_ref() == Some(&self.native_channel)
            && projected(s, &self.channel).is_some_and(|row| row == &self.projected)
    }
}

// Raw canonical booleans are the witnesses here; serde Channel defaults and a
// metadata hit are not substituted for public membership. This first purpose
// deliberately refuses canonical rows lacking explicit private/archive false.
fn ordinary(canonical: &Canonical, channel: &str) -> bool {
    let Some(row) = canonical.row(channel) else {
        return false;
    };
    if row["is_channel"] != true
        || row["is_member"] != true
        || row["is_private"] != false
        || row["is_archived"] != false
        || row
            .get("is_group")
            .is_some_and(|v| !v.is_null() && v != false)
    {
        return false;
    }
    for key in ["is_deleted", "is_tombstone", "is_tombstoned"] {
        if row.get(key).is_some_and(|v| !v.is_null() && v != false) {
            return false;
        }
    }
    canonical::ordinary_label(
        &serde_json::json!({"type":"channel","channel_id":channel}),
        row,
    )
    .is_ok()
        && row["name"]
            .as_str()
            .is_some_and(|name| !name.trim().is_empty())
}
fn projected<'a>(s: &'a ShellState, channel: &str) -> Option<&'a ChannelVm> {
    let mut rows = s.channels.iter().filter(|row| row.id == channel);
    let row = rows.next()?;
    (rows.next().is_none() && !row.is_im && !row.is_mpim && !row.is_private).then_some(row)
}

#[derive(Clone)]
struct Visit {
    view: MainView,
    channel: Option<String>,
    channel_generation: u64,
    thread_generation: u64,
    root: Option<String>,
    core_thread: Option<(String, String)>,
    shortcut: u64,
    reconnect: u64,
    tab: ChannelTab,
    timeline: String,
}
impl Visit {
    fn capture(s: &ShellState) -> Self {
        Self {
            view: s.main_view,
            channel: s.core.active_channel.clone(),
            channel_generation: s.channel_generation,
            thread_generation: s.thread_generation,
            root: s.thread_root.clone(),
            core_thread: s.core.active_thread.clone(),
            shortcut: s.shortcut_generation,
            reconnect: s.reconnect_generation,
            tab: s.channel_tab,
            timeline: s.timeline_scope(),
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        available(s)
            && self.view == s.main_view
            && self.channel == s.core.active_channel
            && self.channel_generation == s.channel_generation
            && self.thread_generation == s.thread_generation
            && self.root == s.thread_root
            && self.core_thread == s.core.active_thread
            && self.shortcut == s.shortcut_generation
            && self.reconnect == s.reconnect_generation
            && self.tab == s.channel_tab
            && self.timeline == s.timeline_scope()
    }
}

/// Purpose-private preparation can be retained while fresh original reads run;
/// it is not callable as navigation. Any observed loss permanently retires it.
#[derive(Clone)]
pub(super) struct Prepared {
    target: Arc<Target>,
    visit: Visit,
    retired: Arc<AtomicBool>,
}
impl Prepared {
    pub(super) fn capture(s: &ShellState, team: &str, channel: &str, ts: &str) -> Option<Self> {
        if !available(s)
            || s.main_view != MainView::Threads
            || !id(team, b'T')
            || !id(channel, b'C')
            || !timestamp(ts)
            || s.core.active_team.as_deref() != Some(team)
            || s.core.active_account.as_deref().is_none_or(str::is_empty)
            || s.core.active_thread.is_some()
        {
            return None;
        }
        let transport = s.core.transport.as_ref()?;
        let session = s.core.session.as_ref()?.workspaces.get(team)?;
        let workspace = s.core.workspaces.get(team)?;
        if session.team_id != team
            || !id(&session.user_id, b'U')
            || workspace.team_id != team
            || workspace.self_user_id != session.user_id
        {
            return None;
        }
        let canonical = canonical::observed(s, team).ok()?;
        let post_boot = Arc::new(cfm_lifecycle::post_boot(s, team).ok()?);
        if !canonical.current_for(s, team)
            || !post_boot.current(s)
            || !ordinary(&canonical, channel)
        {
            return None;
        }
        let native = workspace.channels.get(channel)?;
        let row = canonical.row(channel)?;
        let vm = projected(s, channel)?;
        let native_channel = serde_json::to_value(native).ok()?;
        if native.id != channel
            || !native.is_channel
            || native.is_group
            || native.is_im
            || native.is_mpim
            || native.is_private
            || native.is_archived
            || native.name.as_deref() != row["name"].as_str()
            // Canonical supplied literal true above is the membership witness;
            // a contradictory native field still refuses, without using a
            // serde/default/absent field as the witness.
            || native.extra.get("is_member").is_some_and(|v| v != true)
            || canonical::ordinary_label(
                &serde_json::json!({"type":"channel","channel_id":channel}),
                &native_channel,
            )
            .is_err()
            || vm.name != super_platinum_core::state::channel_display_name(workspace, native)
        {
            return None;
        }
        let prepared = Self {
            target: Arc::new(Target {
                owner: Arc::new(ReadOwner::capture(&s.core, transport, session)?),
                client: s.core.client.clone(),
                canonical,
                post_boot,
                team: team.into(),
                channel: channel.into(),
                ts: ts.into(),
                realtime: workspace.rt_generation,
                lifecycle: workspace.channel_lifecycle.revision(),
                native_channel,
                projected: vm.clone(),
            }),
            visit: Visit::capture(s),
            retired: Arc::new(AtomicBool::new(false)),
        };
        prepared.current(s).then_some(prepared)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let current =
            self.visit.view == MainView::Threads && self.visit.current(s) && self.target.current(s);
        if !current {
            self.retired.store(true, Ordering::Release);
        }
        current
    }
    /// Called ONLY by the timestamp parent after its unique invocation CAS,
    /// under the same Shell borrow and with no await. No watch is queried here.
    pub(super) fn seal(self, s: &ShellState) -> Option<Commit> {
        self.current(s).then_some(Commit { prepared: self })
    }
}

/// Affine, nonclone handoff. Its fields/constructor are private to this purpose.
pub(crate) struct Commit {
    prepared: Prepared,
}
impl Commit {
    pub(crate) fn coordinates(&self, s: &ShellState) -> Option<(&str, &str)> {
        self.prepared.current(s).then_some((
            self.prepared.target.channel.as_str(),
            self.prepared.target.ts.as_str(),
        ))
    }
    /// Capture the genuinely new route immediately after private Global open.
    /// This does not retain/query the outgoing watch or mint reconnect::Visit.
    pub(crate) fn successor(self, s: &ShellState) -> Option<Work> {
        let previous = &self.prepared.visit;
        let target = self.prepared.target;
        if self.prepared.retired.load(Ordering::Acquire)
            || !target.current(s)
            || s.channel_generation != previous.channel_generation.wrapping_add(1)
            || s.thread_generation != previous.thread_generation.wrapping_add(1)
            || s.shortcut_generation != previous.shortcut
            || s.reconnect_generation != previous.reconnect
            || !destination(s, &target)
        {
            return None;
        }
        Some(Work {
            target,
            visit: Visit::capture(s),
        })
    }
}
fn destination(s: &ShellState, target: &Target) -> bool {
    s.main_view == MainView::Home
        && s.channel_tab == ChannelTab::Messages
        && s.core.active_channel.as_deref() == Some(target.channel.as_str())
        && s.thread_root.is_none()
        && s.core.active_thread.is_none()
        && !s.stick_to_bottom
        && s.core
            .pending_scroll_to
            .as_ref()
            .is_some_and(|(channel, anchor)| {
                channel == &target.channel
                    && matches!(anchor, PendingScrollTarget::Message(ts) if ts == &target.ts)
            })
        && s.surfaces.get(&MainView::Home).is_some_and(|surface| {
            surface.channel == target.channel && surface.thread_root.is_none()
        })
}

/// Only this newly captured successor can start existing anchored history work.
pub(crate) struct Work {
    target: Arc<Target>,
    visit: Visit,
}
impl Work {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.visit.current(s) && self.target.current(s) && destination(s, &self.target)
    }
    pub(crate) async fn run_with<F, Fut>(self, state: Signal<ShellState>, refresh: F)
    where
        F: FnOnce(Signal<ShellState>) -> Fut,
        Fut: Future<Output = ()>,
    {
        // Check at first poll, before even constructing refresh's future: that
        // existing function can synchronously capture navigation-history work.
        // Its unchanged history/renderer/currentness/read policy owns awaits.
        let current = state.try_peek().is_ok_and(|s| self.current(&s));
        if current {
            refresh(state).await;
        }
    }
}
