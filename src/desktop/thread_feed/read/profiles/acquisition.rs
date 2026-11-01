//! Private continuity of an already-admitted serial member read. This cannot
//! change Source/Open/Visit generations or create an actor/member permission.
use super::*;
use crate::state::{ChannelTab, Overlay};

#[derive(Clone)]
struct Route {
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    transport: Option<Arc<Transport>>,
    view: MainView,
    channel: Option<String>,
    channel_generation: u64,
    root: Option<String>,
    thread_generation: u64,
    shortcut: u64,
    overlay: Option<Overlay>,
    tab: ChannelTab,
}
impl Route {
    fn capture(s: &ShellState) -> Self {
        Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            transport: s.core.transport.clone(),
            view: s.main_view,
            channel: s.core.active_channel.clone(),
            channel_generation: s.channel_generation,
            root: s.thread_root.clone(),
            thread_generation: s.thread_generation,
            shortcut: s.shortcut_generation,
            overlay: s.overlay.clone(),
            tab: s.channel_tab,
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        self.account == s.core.active_account
            && self.epoch == s.core.account_epoch
            && self.team == s.core.active_team
            && self.view == s.main_view
            && self.channel == s.core.active_channel
            && self.channel_generation == s.channel_generation
            && self.root == s.thread_root
            && self.thread_generation == s.thread_generation
            && self.shortcut == s.shortcut_generation
            && self.overlay == s.overlay
            && self.tab == s.channel_tab
            && match (&self.transport, &s.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn after_profile(&self) -> Option<Self> {
        let mut next = self.clone();
        next.thread_generation = self.thread_generation.checked_add(1)?;
        next.root = None;
        Some(next)
    }
}
#[derive(Clone)]
struct Owner {
    route: Route,
    sequence: u64,
}
#[derive(Default)]
struct Phase {
    owner: Option<Owner>,
    version: u64,
    retired: bool,
}
#[derive(Default)]
pub(super) struct State(Mutex<Phase>);
impl State {
    fn instance(&self, s: &ShellState, scope: &Scope) -> bool {
        std::ptr::eq(self, scope.acquisition.as_ref())
            && s.thread_feed_reads
                .profiles
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), scope))
    }
    pub(super) fn begin(&self, source: &Source, s: &ShellState, scope: &Scope) -> bool {
        // Keep ORIGINAL admission. In particular, a pre-getView Source may
        // have an old Stamp; bind sequence to the actually accepted native page.
        if !source.admitted_current(s)
            || !self.instance(s, scope)
            || !scope.current(s)
            || source.team != scope.team
            || source.epoch != scope.epoch
            || !Arc::ptr_eq(&source.root, &scope.origin)
            || !source.client.same_configuration(&scope.client)
            || s.core.threads_view.loading
            || s.thread_feed_reads.owned_sequence != Some(s.core.threads_view.load_seq)
        {
            return false;
        }
        let Ok(mut p) = self.0.lock() else {
            return false;
        };
        if p.retired || p.owner.is_some() {
            return false;
        }
        p.owner = Some(Owner {
            route: Route::capture(s),
            sequence: s.core.threads_view.load_seq,
        });
        true
    }
    pub(super) fn current(&self, s: &ShellState, scope: &Scope) -> bool {
        // A completed own immutable publication replaces its Scope pointer.
        // Old callbacks fail, but cannot poison the successor's shared owner.
        if !self.instance(s, scope) {
            return false;
        }
        let namespace = scope.current(s);
        let Ok(mut p) = self.0.lock() else {
            return false;
        };
        if p.retired {
            return false;
        }
        let Some(owner) = p.owner.as_ref() else {
            return false;
        };
        let current = namespace
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == owner.sequence
            && s.thread_feed_reads.owned_sequence == Some(owner.sequence)
            && owner.route.current(s);
        if !current {
            p.retired = true;
        }
        current
    }
}

pub(super) struct OpenPlan {
    open: Open,
    owner: Owner,
    version: u64,
}
pub(super) fn before_open(open: &Open, s: &ShellState) -> Option<OpenPlan> {
    if !open.current(s) || !open.scope.acquisition.current(s, &open.scope) {
        return None;
    }
    let p = open.scope.acquisition.0.lock().ok()?;
    if p.retired {
        return None;
    }
    Some(OpenPlan {
        open: open.clone(),
        owner: p.owner.clone()?,
        version: p.version,
    })
}
impl OpenPlan {
    pub(super) fn finish(self, s: &ShellState, opened: bool) {
        let scope = &self.open.scope;
        // true means show_member actually installed this owned member snapshot.
        // False can occur AFTER close_thread; it must never bless that mutation.
        let valid = opened
            && self
                .owner
                .route
                .after_profile()
                .is_some_and(|r| r.current(s))
            && scope.current(s)
            && scope.actor_current(s, &self.open.key)
            && !self.open.member.retired.load(Ordering::Acquire)
            && scope
                .members
                .get(&self.open.member.user().id)
                .is_some_and(|m| Arc::ptr_eq(m, &self.open.member))
            && s.profile_user.as_ref() == Some(&self.open.member.user().id)
            && s.core
                .profile_pane
                .as_ref()
                .is_some_and(|p| p.user == self.open.member.user().id)
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == self.owner.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.owner.sequence);
        let Ok(mut p) = scope.acquisition.0.lock() else {
            return;
        };
        if p.retired || p.version != self.version {
            return;
        }
        let Some(version) = p.version.checked_add(1) else {
            p.retired = true;
            return;
        };
        if !valid {
            p.retired = true;
            return;
        }
        let Some(route) = self.owner.route.after_profile() else {
            p.retired = true;
            return;
        };
        p.owner = Some(Owner {
            route,
            sequence: self.owner.sequence,
        });
        p.version = version;
    }
}
