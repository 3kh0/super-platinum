//! A distinct original Root first-Add purpose, never an existing-pill grant.
use super::*;
use crate::state::Overlay;
use super_platinum_core::thread_reaction::{
    RootMembership, RootPolicy,
    first_add_alias::{AliasDomain, AliasQuery},
    first_add_preferred::{PreferredChoice, PreferredDomain},
};
mod custom;
mod name_search;
pub(crate) use custom::{
    ImageDisplay, image_assets, retry as retry_custom, set_mode as set_custom_mode,
};
mod picker;
mod roster;
mod run;
mod selection;
pub(crate) use picker::{
    Choice, Presentation, choose, choose_active, close, close_shell, move_cursor, presentation,
    retry, set_query, show_more,
};
pub(in super::super) use roster::Sink;
use roster::{Coordinate, Scope};
pub(super) use roster::{barrier, connection_changed, event};
use selection::Selected;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
#[derive(Default)]
pub(crate) struct State {
    scope: Option<Arc<Scope>>,
    session: Option<Session>,
    active: Option<Arc<Intent>>,
    layout: bool,
    custom_layout: bool,
    ui_epoch: u64,
}
struct Session {
    owner: Arc<Picker>,
    ready: Option<Arc<Prepared>>,
    query: String,
    frame: u64,
    cursor: usize,
    limit: usize,
    matches: Vec<usize>,
    loading: bool,
    notice: String,
    custom: custom::State,
}
struct Prepared {
    loaded: Arc<crate::emoji_catalog::Loaded>,
    domain: PreferredDomain,
    alias_domain: AliasDomain,
    choices: Vec<Selected>,
    name_model: name_search::Model,
    _policy: RootPolicy,
    _membership: Arc<RootMembership>,
}
struct Picker {
    id: u64,
    open: Open,
    alive: AtomicBool,
    attempt: AtomicU64,
    chosen: AtomicBool,
}
impl Picker {
    fn current(&self, s: &ShellState) -> bool {
        self.alive.load(Ordering::Acquire)
            && !self.chosen.load(Ordering::Acquire)
            && s.overlay == Some(Overlay::ThreadRootFirstAdd)
            && self.open.base_current(s)
            && s.thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()
                .is_some_and(|p| p.owner.id == self.id)
    }
    fn preparing(&self, s: &ShellState, attempt: u64) -> bool {
        self.current(s) && self.attempt.load(Ordering::Acquire) == attempt
    }
}
#[derive(Clone)]
pub(crate) struct Open {
    scope: Arc<Scope>,
    key: Coordinate,
    source: Source,
    channel: u64,
    edit: u64,
    shortcut: u64,
    ui: u64,
}
impl PartialEq for Open {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &b.scope)
            && self.key == b.key
            && self.source.stamp == b.source.stamp
            && self.channel == b.channel
            && self.edit == b.edit
            && self.shortcut == b.shortcut
            && self.ui == b.ui
    }
}
impl Open {
    fn base_current(&self, s: &ShellState) -> bool {
        self.source.current(s)
            && s.core.editing.is_none()
            && s.channel_generation == self.channel
            && s.edit_generation == self.edit
            && s.shortcut_generation == self.shortcut
            && self.scope.row_current(s, &self.key, true)
    }
    pub(crate) fn id(&self) -> String {
        format!(
            "thread-root-first-add-open-{}-{}-{}",
            self.scope.id,
            self.key.0,
            self.key.1.replace('.', "")
        )
    }
    pub(crate) fn disabled(&self, s: &ShellState) -> bool {
        !self.base_current(s)
            || s.overlay.is_some()
            || self.ui != s.thread_feed_reads.reaction_toggle.first_add.ui_epoch
            || self.ui == u64::MAX
            || !self.scope.caught_up(s)
            || crate::reaction_writes::registry(s).blocked(&self.scope.physical(&self.key))
    }
    pub(crate) fn start(&self, mut state: Signal<ShellState>) {
        let work = {
            let mut s = state.write();
            if self.disabled(&s) {
                return;
            }
            let Some(id) = super::next() else { return };
            let Some(ui) = s
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .ui_epoch
                .checked_add(1)
            else {
                return;
            };
            let p = Arc::new(Picker {
                id,
                open: self.clone(),
                alive: AtomicBool::new(true),
                attempt: AtomicU64::new(1),
                chosen: AtomicBool::new(false),
            });
            s.thread_feed_reads.reaction_toggle.first_add.session = Some(Session {
                owner: p.clone(),
                ready: None,
                custom: custom::State::default(),
                query: String::new(),
                frame: 1,
                cursor: 0,
                limit: 120,
                matches: vec![],
                loading: true,
                notice: "Checking this message and loading emoji…".into(),
            });
            s.thread_feed_reads.reaction_toggle.first_add.ui_epoch = ui;
            s.overlay = Some(Overlay::ThreadRootFirstAdd);
            p
        };
        dioxus::core::spawn_forever(run::prepare(state, work, 1));
    }
}
pub(crate) fn opener(source: &Source, s: &ShellState, item: &ThreadViewItem) -> Option<Open> {
    let scope = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .scope
        .clone()?;
    let key = roster::coordinate(item)?;
    let open = Open {
        scope,
        key,
        source: source.clone(),
        channel: s.channel_generation,
        edit: s.edit_generation,
        shortcut: s.shortcut_generation,
        ui: s.thread_feed_reads.reaction_toggle.first_add.ui_epoch,
    };
    (open.base_current(s)
        && matches!(s.overlay, None | Some(Overlay::ThreadRootFirstAdd))
        && open.scope.caught_up(s))
    .then_some(open)
}
pub(in super::super) fn publish(source: &Source, s: &mut ShellState, sink: &Sink) {
    roster::publish(source, s, sink.take());
}
struct Intent {
    id: u64,
    open: Open,
    prepared: Arc<Prepared>,
    selected: Selected,
    ledger: Registry,
    phase: std::sync::atomic::AtomicU8,
    notice: Mutex<String>,
}
impl Intent {
    fn current(&self, s: &ShellState) -> bool {
        self.open.base_current(s)
            && s.overlay.is_none()
            && s.thread_feed_reads
                .reaction_toggle
                .first_add
                .active
                .as_ref()
                .is_some_and(|p| p.id == self.id)
            && self.selected.native_current(s)
    }
    fn notice(&self, text: &str) {
        if let Ok(mut n) = self.notice.lock() {
            *n = text.into();
        }
    }
}
pub(crate) fn status(s: &ShellState, item: &ThreadViewItem) -> Option<String> {
    let scope = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .scope
        .as_ref()?;
    let key = roster::coordinate(item)?;
    if !scope.row_current(s, &key, false) {
        return None;
    }
    if let Some(p) = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .active
        .as_ref()
        .filter(|p| p.open.key == key && Arc::ptr_eq(&p.open.scope, scope))
    {
        return p.notice.lock().ok().map(|s| s.clone());
    }
    crate::reaction_writes::registry(s).blocked(&scope.physical(&key)).then(||"An earlier reaction is pending, unconfirmed or cooling down. Refresh reads current reactions; it never resends or unblocks the operation.".into())
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.thread_feed_reads.reaction_toggle.first_add.layout = true;
    s.overlay = Some(Overlay::ThreadRootFirstAdd);
}
pub(crate) fn layout(s: &ShellState) -> bool {
    s.thread_feed_reads.reaction_toggle.first_add.layout
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.active_account.is_none()
}

pub(crate) fn custom_fixture(s: &mut ShellState) {
    fixture(s);
    s.thread_feed_reads.reaction_toggle.first_add.custom_layout = true;
}
