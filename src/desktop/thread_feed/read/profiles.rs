//! Profile-specific original actor/member custody. Pane-open Accepted is NOT a grant.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Mutex,
};
use super_platinum_core::{
    profile_reading::Projection,
    slack::models::{Message, ThreadViewItem},
};
mod acquisition;
mod deferred;
mod members;
pub(super) mod network;
#[cfg(test)]
pub(super) mod test_io;
#[cfg(test)]
mod tests;

type Key = (String, String, String);
/// Private original member version. Retirement only removes reading authority.
pub(crate) struct Member {
    value: Projection,
    retired: AtomicBool,
}
impl Member {
    fn new(value: Projection) -> Self {
        Self {
            value,
            retired: AtomicBool::new(false),
        }
    }
    fn user(&self) -> &super_platinum_core::slack::models::User {
        self.value.user()
    }
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
    }
}
/// A current original raw profile read may replace only its matching original
/// member version. A bare data Projection, cached User or foreign pane cannot
/// invoke this publication path.
pub(crate) fn publish_refresh(
    shell: &mut ShellState,
    proof: crate::bootstrap::profile_search::Refreshed<'_>,
) -> Option<Arc<Member>> {
    let scope = shell.thread_feed_reads.profiles.as_ref()?.clone();
    if !scope.current(shell) {
        return None;
    }
    let (team, original, value) = proof.details(shell)?;
    let id = &value.user().id;
    if team != scope.team
        || original.retired.load(Ordering::Acquire)
        || original.user().id != *id
        || !scope
            .members
            .get(id)
            .is_some_and(|m| Arc::ptr_eq(m, original))
        || !scope
            .actors
            .iter()
            .any(|(key, actor)| actor.user == *id && scope.actor_current(shell, key))
    {
        return None;
    }
    let member = Arc::new(Member::new(value.clone()));
    let mut next = scope.as_ref().clone();
    next.members.insert(id.clone(), member.clone());
    next.retired = Arc::new(AtomicBool::new(false));
    // Immutable publication invalidates all old header callbacks. Other original
    // actors/members stay owned, and Core's canonical users remain untouched.
    shell.thread_feed_reads.profiles = Some(Arc::new(next));
    Some(member)
}
#[derive(Clone, PartialEq, Eq)]
struct Actor {
    user: String,
    kind: Option<String>,
    team: Option<String>,
    channel: Option<String>,
    thread: Option<String>,
}
fn timestamp(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once('.')?;
    if a.is_empty()
        || b.is_empty()
        || b.len() > 6
        || !a.bytes().chain(b.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let a = a.parse::<u64>().ok()?;
    (a > 0 && a <= i64::MAX as u64)
        .then_some((a, b.parse::<u64>().ok()? * 10u64.pow(6 - b.len() as u32)))
}
fn actor(message: &Message, team: &str, channel: &str, root: &str) -> Option<Actor> {
    let user = message.user.as_ref()?;
    let ts = message.ts.as_deref()?;
    timestamp(ts)?;
    if !super_platinum_core::profile_reading::valid_id(user)
        || message.kind.as_deref() != Some("message")
        || message.subtype.is_some()
        || message.bot_id.is_some()
        || message.bot_profile.is_some()
        // iOp7 resolves document editors/authors separately from msg.user.
        // Their passive labels never inherit this raw-user profile purpose.
        || message.extra.contains_key("document_comment")
        || message.channel.as_deref().is_some_and(|v| v != channel)
        || message.thread_ts.as_deref().is_some_and(|v| v != root)
        || message.team.as_deref().is_some_and(|v| v != team)
        || super_platinum_core::channel_threads::hidden(message)
    {
        return None;
    }
    Some(Actor {
        user: user.clone(),
        kind: message.kind.clone(),
        // Missing optional message context is resolved only by this original
        // native page's bound namespace/key; explicit conflicts were refused.
        team: Some(team.into()),
        channel: Some(channel.into()),
        thread: Some(root.into()),
    })
}
pub(super) struct Candidates(BTreeMap<Key, Actor>, BTreeSet<Key>);
impl Candidates {
    /// Called on the ORIGINAL successful page before public Core publication.
    pub(super) fn from_page(page: &ThreadsViewPage, team: &str) -> Self {
        let mut rows = BTreeMap::<Key, Option<Actor>>::new();
        for item in &page.threads {
            let (Some(channel), Some(root)) = (
                item.root_msg.channel.as_deref(),
                item.root_msg.ts.as_deref(),
            ) else {
                continue;
            };
            if channel.is_empty() || timestamp(root).is_none() {
                continue;
            }
            for msg in std::iter::once(&item.root_msg)
                .chain(&item.latest_replies)
                .chain(&item.unread_replies)
            {
                let Some(ts) = msg.ts.clone() else { continue };
                let value = actor(msg, team, channel, root);
                let key = (channel.to_owned(), root.to_owned(), ts);
                rows.entry(key)
                    .and_modify(|old| {
                        if old.as_ref() != value.as_ref() {
                            *old = None
                        }
                    })
                    .or_insert(value);
            }
        }
        let touched = rows.keys().cloned().collect();
        Self(
            rows.into_iter()
                .filter_map(|(k, v)| v.map(|v| (k, v)))
                .collect(),
            touched,
        )
    }
}
#[derive(Clone)]
pub(super) struct Scope {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
    actors: BTreeMap<Key, Actor>,
    network: network::Bindings,
    deferred: deferred::Roster,
    members: HashMap<String, Arc<Member>>,
    started: Arc<AtomicBool>,
    acquisition: Arc<acquisition::State>,
    retired: Arc<AtomicBool>,
    retired_actors: Arc<Mutex<HashSet<Key>>>,
}
impl Scope {
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        candidates: Candidates,
        append: bool,
    ) -> Option<Self> {
        if source.session.enterprise_id.is_some()
            || source.session.team_id != source.team
            || !source.client.same_configuration(&shell.core.client)
            || !source.admitted_current(shell)
            || !shell.core.threads_view.loaded
            || shell.core.threads_view.loading
            || shell.thread_feed_reads.owned_sequence != Some(shell.core.threads_view.load_seq)
        {
            return None;
        }
        // Only an original native append can retain previous original bindings
        // and member receipts; a public loaded vector supplies neither.
        let previous = shell
            .thread_feed_reads
            .profiles
            .as_ref()
            .filter(|old| append && old.current(shell));
        // A fresh getView intake never converts an earlier-read binding into
        // an original-page binding. Only its own incoming raw actors can do so.
        let mut original = previous
            .map(|old| {
                old.actors
                    .iter()
                    .filter(|(key, _)| !old.network.contains_key(*key))
                    .map(|(key, actor)| (key.clone(), actor.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let mut retired_actors = match previous {
            Some(old) => old.retired_actors.lock().ok()?.clone(),
            None => HashSet::new(),
        };
        for key in &candidates.1 {
            original.remove(key);
            retired_actors.remove(key);
        }
        let incoming = candidates.0;
        original.extend(incoming.clone());
        let actors = original
            .into_iter()
            .filter(|(key, a)| {
                current_message(shell, key)
                    .is_some_and(|m| actor(m, &source.team, &key.0, &key.1).as_ref() == Some(a))
            })
            .collect::<BTreeMap<_, _>>();
        if actors.is_empty() {
            return None;
        }
        let deferred = deferred::Roster::capture(
            shell,
            &source.team,
            previous.map(|old| &old.deferred),
            &incoming,
            &candidates.1,
            &actors,
        )?;
        Some(Self {
            origin: source.root.clone(),
            owner: source.owner.clone(),
            cursor_owner: source.cursor_owner.clone(),
            client: source.client.clone(),
            epoch: source.epoch,
            team: source.team.clone(),
            actors,
            network: BTreeMap::new(),
            deferred,
            members: previous.map(|old| old.members.clone()).unwrap_or_default(),
            started: Arc::new(AtomicBool::new(false)),
            acquisition: Arc::new(acquisition::State::default()),
            retired: Arc::new(AtomicBool::new(false)),
            retired_actors: Arc::new(Mutex::new(retired_actors)),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let current = shell.signed_in
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &shell.thread_feed_reads.incarnation)
            && self.owner.current(&shell.core)
            && self.cursor_owner.current(&shell.core)
            && self.client.same_configuration(&shell.core.client)
            && shell
                .core
                .threads_view
                .matches_scope(self.epoch, &self.team)
            && shell.core.threads_view.loaded
            && shell.thread_feed_reads.owned_sequence == Some(shell.core.threads_view.load_seq)
            && shell
                .thread_feed_reads
                .profiles
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self));
        if !current {
            self.retired.store(true, Ordering::Release);
        }
        current
    }
    fn actor_current(&self, shell: &ShellState, key: &Key) -> bool {
        if !self.current(shell) {
            return false;
        }
        let Ok(mut retired) = self.retired_actors.lock() else {
            return false;
        };
        if retired.contains(key) {
            return false;
        }
        let current = self.network.get(key).is_none_or(|p| {
            p.current(shell) && current_message(shell, key).is_some_and(network::ordinary)
        }) && self.actors.get(key).is_some_and(|a| {
            current_message(shell, key)
                .is_some_and(|m| actor(m, &self.team, &key.0, &key.1).as_ref() == Some(a))
        });
        if !current {
            retired.insert(key.clone());
        }
        current
    }
}
fn current_message<'a>(shell: &'a ShellState, key: &Key) -> Option<&'a Message> {
    let mut found = shell.core.threads_view.items.iter().filter(|i| {
        i.root_msg.channel.as_ref() == Some(&key.0) && i.root_msg.ts.as_ref() == Some(&key.1)
    });
    let item = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let team = shell.core.active_team.as_ref()?;
    if shell
        .core
        .workspaces
        .get(team)?
        .messages
        .get(&key.0)
        .is_some_and(|b| b.was_deleted(&key.2))
    {
        return None;
    }
    if key.2 == key.1 {
        return Some(&item.root_msg);
    }
    // Exactly the existing bounded, hidden/deletion/scope-aware reply selection.
    crate::view::selected_thread_replies(shell, item)
        .into_iter()
        .find(|m| m.ts.as_ref() == Some(&key.2))
}

#[derive(Clone)]
pub(crate) struct Open {
    source: Source,
    scope: Arc<Scope>,
    key: Key,
    member: Arc<Member>,
}
impl Open {
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        item: &ThreadViewItem,
        message: &Message,
    ) -> Option<Self> {
        let scope = shell.thread_feed_reads.profiles.as_ref()?.clone();
        if !scope.current(shell) || !source.current(shell) || shell.overlay.is_some() {
            return None;
        }
        let key = (
            item.root_msg.channel.clone()?,
            item.root_msg.ts.clone()?,
            message.ts.clone()?,
        );
        if !scope.actor_current(shell, &key) {
            return None;
        }
        let id = &scope.actors.get(&key)?.user;
        let member = scope.members.get(id)?.clone();
        let open = Self {
            source: source.clone(),
            scope,
            key,
            member,
        };
        open.current(shell).then_some(open)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && self.source.current(shell)
            && shell.overlay.is_none()
            && self.scope.actor_current(shell, &self.key)
            && !self.member.retired.load(Ordering::Acquire)
            // Public metadata may deny an opener; it can never create one.
            && shell.core.workspaces.get(&self.scope.team).is_some_and(|w|w.users.get(&self.member.user().id).is_none_or(|u| {
                u.id==self.member.user().id && !u.is_bot && !u.deleted
                    && u.extra.get("is_app_user").is_none_or(|v|v.is_null()||v==&serde_json::json!(false))
            }))
            && self
                .scope
                .actors
                .get(&self.key)
                .is_some_and(|a| a.user == self.member.user().id)
            && self
                .scope
                .members
                .get(&self.member.user().id)
                .is_some_and(|v| Arc::ptr_eq(v, &self.member))
    }
    pub(crate) fn user(&self, shell: &ShellState) -> Option<&Projection> {
        self.current(shell).then_some(&self.member.value)
    }
    pub(crate) fn reading_context(
        &self,
        shell: &ShellState,
    ) -> Option<(
        String,
        Arc<crate::bootstrap::ReadOwner>,
        SlackClient,
        Arc<Member>,
    )> {
        self.current(shell).then(|| {
            (
                self.scope.team.clone(),
                self.scope.owner.clone(),
                self.scope.client.clone(),
                self.member.clone(),
            )
        })
    }
    pub(crate) fn start(&self, mut state: Signal<ShellState>) {
        // The independent read-only pane is committed and ProfileVisit bound
        // before queuing. This never calls the canonical/global profile opener.
        if let Ok(mut shell) = state.try_write() {
            let plan = acquisition::before_open(self, &shell);
            let opened = crate::bootstrap::profile_search::show_member(&mut shell, self);
            if let Some(plan) = plan {
                plan.finish(&shell, opened);
            }
        }
    }
    pub(crate) fn control_id(&self) -> String {
        format!(
            "thread-profile-{}-{}-{}",
            self.key.0, self.key.1, self.key.2
        )
    }
}
pub(super) async fn load(source: Source, state: Signal<ShellState>) {
    members::load(source, state).await;
}
#[cfg(test)]
pub(super) async fn fixture_member_read(
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<serde_json::Value, Error> {
    match test_io::current() {
        Some(io) => io.execute(request).await,
        None => Err(Error::Offline("No scoped document-label fixture IO".into())),
    }
}
/// Display data from an existing genuine human member occurrence only. This
/// returns a label, never an actor binding, opener, canonical user or IO grant.
pub(super) fn reading_name(source: &Source, shell: &ShellState, id: &str) -> Option<String> {
    if !source.current(shell) {
        return None;
    }
    let scope = shell.thread_feed_reads.profiles.as_ref()?;
    if !scope.current(shell)
        || source.team != scope.team
        || !source.client.same_configuration(&scope.client)
    {
        return None;
    }
    let member = scope.members.get(id)?;
    if member.retired.load(Ordering::Acquire)
        || member.user().id != id
        || shell
            .core
            .workspaces
            .get(&scope.team)?
            .users
            .get(id)
            .is_some_and(|u| {
                u.id != id
                    || u.is_bot
                    || u.deleted
                    || u.extra
                        .get("is_app_user")
                        .is_some_and(|v| !v.is_null() && v != &serde_json::json!(false))
            })
    {
        return None;
    }
    Some(super_platinum_core::state::display_name(
        Some(member.user()),
        id,
    ))
}
pub(super) fn after_local_reveal(shell: &mut ShellState, key: (&str, &str)) -> bool {
    deferred::activate_visible(shell, key)
}
pub(super) fn retire_deferred(shell: &ShellState) {
    if let Some(scope) = shell.thread_feed_reads.profiles.as_ref() {
        if scope.current(shell) {
            scope.deferred.retire_missing(shell, &scope.team);
            for key in scope.network.keys() {
                scope.actor_current(shell, key);
            }
        }
    }
}
