//! Search-only human reading. No indexed-name or canonical-user grant.
use super::*;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack::realtime::{ReadBarrier, ReadWatch};
mod owner;
mod pane;
mod run;
use owner::Context;
pub(crate) use pane::{Back, Open, Pane, back, open};
pub(crate) use pane::{Refreshed, refresh as pane_refresh};
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(test)]
mod tests;
type Key = (String, String);
const LIMIT: usize = 100; // Native bound, not a Slack schema.
#[derive(Default)]
pub(crate) struct Registry {
    applied: BTreeMap<String, (u64, ReadBarrier)>,
    connected: BTreeMap<String, Arc<crate::bootstrap::ReadOwner>>,
    epoch: Arc<()>,
    events: BTreeSet<String>,
    cooldowns: BTreeMap<
        String,
        (
            Arc<crate::bootstrap::ReadOwner>,
            Option<std::time::Instant>,
            bool,
        ),
    >,
}
#[derive(Default)]
pub(super) struct State {
    pages: BTreeMap<u32, Arc<Roster>>,
    prepared: Option<Arc<Prepared>>,
    waiting: bool,
}
pub(in crate::bootstrap::search) struct RawRoster(BTreeMap<Key, Option<String>>);
struct Actor {
    key: Key,
    id: String,
    retired: AtomicBool,
}
struct Roster {
    actors: BTreeMap<Key, Arc<Actor>>,
    attempted: AtomicBool,
    automatic: AtomicBool,
}
struct Prepared {
    context: Context,
    roster: Arc<Roster>,
    members: BTreeMap<String, super_platinum_core::profile_reading::Projection>,
    hidden: BTreeSet<String>,
    pending: bool,
    failed: bool,
    retry_at: Option<std::time::Instant>,
    blocked: bool,
}
fn member_id(id: &str) -> bool {
    (id.starts_with('U') || id.starts_with('W'))
        && id.len() >= 2
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && !matches!(id, "USLACK" | "USLACKBOT")
}
fn falsey(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null) | Some(Value::Bool(false)))
        || v.is_some_and(|v| v.as_str() == Some(""))
}
fn timestamp(ts: &str) -> bool {
    let Some((a, b)) = ts.split_once('.') else {
        return false;
    };
    a.len() == 10
        && b.len() == 6
        && a.bytes().chain(b.bytes()).all(|b| b.is_ascii_digit())
        && a.parse::<u64>().is_ok_and(|n| n > 0)
}
/// Passive data before typed decode; only original Read publication installs it.
pub(in crate::bootstrap::search) fn capture_raw(raw: &Value, team: &str) -> RawRoster {
    let mut rows = BTreeMap::new();
    if raw.get("ok") != Some(&Value::Bool(true)) || raw.get("error").is_some_and(|v| !v.is_null()) {
        return RawRoster(rows);
    }
    let Some(items) = raw
        .get("items")
        .and_then(Value::as_array)
        .filter(|a| a.len() <= LIMIT)
    else {
        return RawRoster(rows);
    };
    for item in items {
        let Some(channel) = item
            .get("channel")
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            .filter(|s| {
                s.as_bytes()
                    .first()
                    .is_some_and(|b| matches!(*b, b'C' | b'G' | b'D'))
                    && s.len() >= 2
                    && s.len() <= 64
                    && s.bytes().all(|b| b.is_ascii_alphanumeric())
            })
        else {
            continue;
        };
        let Some(messages) = item
            .get("messages")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 1)
        else {
            continue;
        };
        let m = &messages[0];
        let Some(ts) = m.get("ts").and_then(Value::as_str).filter(|s| timestamp(s)) else {
            continue;
        };
        let key = (channel.to_owned(), ts.to_owned());
        let id = (|| {
            let id = m.get("user")?.as_str()?;
            if !member_id(id)
                || m.get("type").and_then(Value::as_str) != Some("message")
                || item.get("team").is_some_and(|v| v.as_str() != Some(team))
                || m.get("team").is_some_and(|v| v.as_str() != Some(team))
                || m.get("channel")
                    .is_some_and(|v| v.as_str() != Some(channel))
                || [
                    "bot_id",
                    "bot_profile",
                    "subtype",
                    "message",
                    "document_comment",
                    "comment",
                    "file",
                    "ai_context",
                    "workflow_id",
                    "app_id",
                    "room_id",
                    "tab_added_user_id",
                    "item_added_user_id",
                    "isUnknown",
                    "isNonExistent",
                    "isEKMError",
                    "is_ephemeral",
                    "is_deleted",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                ]
                .iter()
                .any(|k| !falsey(m.get(*k)))
                || ["files", "attachments"].iter().any(|k| {
                    m.get(*k)
                        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
                })
            {
                return None;
            }
            Some(id.to_owned())
        })();
        if rows.contains_key(&key) {
            rows.insert(key, None);
        } else {
            rows.insert(key, id);
        }
    }
    RawRoster(rows)
}
pub(in crate::bootstrap::search) fn install(s: &mut ShellState, raw: RawRoster) {
    let Some(b) = s.search_message_pages.as_mut() else {
        return;
    };
    let Some(rows) = b.pages.get(&b.shown) else {
        return;
    };
    let actors = raw
        .0
        .into_iter()
        .filter_map(|(key, id)| {
            let id = id?;
            (rows
                .iter()
                .filter(|r| r.channel_id == key.0 && r.ts == key.1)
                .count()
                == 1)
                .then(|| {
                    let a = Arc::new(Actor {
                        key: key.clone(),
                        id,
                        retired: AtomicBool::new(false),
                    });
                    (key, a)
                })
        })
        .collect();
    b.profiles.pages.insert(
        b.shown,
        Arc::new(Roster {
            actors,
            attempted: AtomicBool::new(false),
            automatic: AtomicBool::new(true),
        }),
    );
    b.profiles.prepared = None;
    b.profiles.waiting = true;
}
fn actor_current(s: &ShellState, p: &Prepared, a: &Arc<Actor>) -> bool {
    p.context.base(s) && actor_serial(s, p, a)
}
fn actor_serial(s: &ShellState, p: &Prepared, a: &Arc<Actor>) -> bool {
    !a.retired.load(Ordering::Acquire)
        && p.context.serial(s)
        && !known_denied(s, &p.context.frame.team, a)
        && s.search_message_pages.as_ref().is_some_and(|b| {
            b.profiles.pages.get(&p.context.page).is_some_and(|r| {
                Arc::ptr_eq(r, &p.roster) && r.actors.get(&a.key).is_some_and(|v| Arc::ptr_eq(v, a))
            }) && !b.pages.get(&b.shown).is_some_and(|rows| {
                rows.iter().enumerate().any(|(i, r)| {
                    r.channel_id == a.key.0 && r.ts == a.key.1 && known_deleted(s, b, i)
                })
            })
        })
}
fn ready(s: &ShellState, p: &Prepared) -> bool {
    !p.pending && !p.failed && p.context.current(s)
}
#[derive(Clone)]
pub(crate) struct Control {
    row: RowSource,
    prepared: Arc<Prepared>,
    actor: Arc<Actor>,
    used: Arc<AtomicBool>,
}
pub(crate) fn control(s: &ShellState, index: usize) -> Option<Control> {
    let row = RowSource::capture(s, index)?;
    let hit = row.resolve(s)?;
    let p = s
        .search_message_pages
        .as_ref()?
        .profiles
        .prepared
        .as_ref()?
        .clone();
    let actor = p.roster.actors.get(&(hit.channel_id, hit.ts))?.clone();
    if !ready(s, &p)
        || !actor_current(s, &p, &actor)
        || !p.members.contains_key(&actor.id)
        || p.hidden.contains(&actor.id)
    {
        return None;
    }
    Some(Control {
        row,
        prepared: p,
        actor,
        used: Arc::new(AtomicBool::new(false)),
    })
}
impl Control {
    fn current(&self, s: &ShellState) -> bool {
        !self.used.load(Ordering::Acquire)
            && self
                .row
                .resolve(s)
                .is_some_and(|r| (r.channel_id, r.ts) == self.actor.key)
            && ready(s, &self.prepared)
            && actor_current(s, &self.prepared, &self.actor)
            && s.search_message_pages
                .as_ref()
                .and_then(|b| b.profiles.prepared.as_ref())
                .is_some_and(|p| Arc::ptr_eq(p, &self.prepared))
    }
}
#[derive(Clone)]
pub(crate) struct Retry {
    prepared: Arc<Prepared>,
    dialog: u64,
}
pub(crate) fn status(s: &ShellState) -> Option<(&'static str, Option<Retry>)> {
    if s.overlay != Some(Overlay::Search) {
        return None;
    }
    let p = s
        .search_message_pages
        .as_ref()?
        .profiles
        .prepared
        .as_ref()?;
    if !p.context.base(s) {
        return None;
    }
    if p.pending {
        return Some(("Preparing author profiles…", None));
    }
    if p.failed || !p.context.policy_current(s) {
        return Some((
            "Author profiles are unavailable. Search results remain available.",
            Some(Retry {
                prepared: p.clone(),
                dialog: s.search_open_generation,
            }),
        ));
    }
    None
}
pub(crate) fn retry(state: Signal<ShellState>, token: Retry) {
    let valid = {
        let s = state.peek();
        s.overlay == Some(Overlay::Search)
            && s.search_open_generation == token.dialog
            && token.prepared.context.base(&s)
            && s.search_message_pages
                .as_ref()
                .and_then(|b| b.profiles.prepared.as_ref())
                .is_some_and(|p| Arc::ptr_eq(p, &token.prepared))
    };
    if valid {
        prepare(state, true);
    }
}
#[cfg(test)]
fn model_io() -> Option<std::rc::Rc<test_io::Io>> {
    let runtime = dioxus::core::Runtime::try_current()?;
    runtime.try_current_scope_id()?;
    try_consume_context()
}
pub(in crate::bootstrap::search) fn prepare(mut state: Signal<ShellState>, explicit: bool) {
    #[cfg(test)]
    let io = model_io();
    #[cfg(test)]
    if io.is_none() {
        return;
    }
    let work = state
        .try_write()
        .ok()
        .and_then(|mut s| run::begin(&mut s, explicit));
    if let Some(work) = work {
        #[cfg(test)]
        spawn(async move {
            run::run(state, work, io).await;
        });
        #[cfg(not(test))]
        spawn(async move {
            run::run(state, work).await;
        });
    }
}
pub(crate) fn accept_barrier(
    s: &mut ShellState,
    team: &str,
    generation: u64,
    barrier: ReadBarrier,
) {
    if s.core
        .workspaces
        .get(team)
        .is_some_and(|w| w.rt_generation == generation)
        && owner::connection(s, team).is_some_and(|c| barrier.belongs_to(c))
    {
        // Initial receipt is accepted only from the real ReadBarrier receiver.
        if s.search_author_profiles.applied.contains_key(team)
            && !s.search_author_profiles.events.remove(team)
        {
            invalidate(s, team, false);
        }
        s.search_author_profiles.events.remove(team);
        s.search_author_profiles
            .applied
            .insert(team.into(), (generation, barrier));
    }
}
/// Called only after the actual barrier receiver releases its Shell borrow.
/// This completes a never-started publication; it never retries an old attempt.
pub(crate) fn after_barrier(state: Signal<ShellState>, team: &str, generation: u64) {
    let prime = state.try_read().ok().is_some_and(|s| {
        let Some(b) = s.search_message_pages.as_ref() else {
            return false;
        };
        b.frame.team == team
            && b.profiles.prepared.is_none()
            && b.profiles.waiting
            && s.core
                .workspaces
                .get(team)
                .is_some_and(|w| w.rt_generation == generation)
            && b.profiles.pages.get(&b.shown).is_some_and(|r| {
                r.actors
                    .values()
                    .any(|a| !a.retired.load(Ordering::Acquire))
            })
            && Context::capture(&s).is_some()
    });
    if prime {
        prepare(state, false);
    }
}
/// Only the accepted Connected receiver calls this. A first original account
/// connection may inherit passive, never-attempted raw data, never old actions.
pub(crate) fn connected(s: &mut ShellState, team: &str) {
    let incoming = s
        .core
        .transport
        .as_ref()
        .zip(
            s.core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(team)),
        )
        .and_then(|(transport, session)| {
            crate::bootstrap::ReadOwner::capture(&s.core, transport, session)
        })
        .map(Arc::new);
    let first = incoming.is_some()
        && !s
            .search_author_profiles
            .connected
            .get(team)
            .is_some_and(|owner| owner.current(&s.core));
    if let Some(owner) = incoming {
        s.search_author_profiles
            .connected
            .insert(team.into(), owner);
    }
    s.search_author_profiles.applied.remove(team);
    s.search_author_profiles.events.remove(team);
    if !s
        .search_message_pages
        .as_ref()
        .is_some_and(|b| b.frame.team == team)
    {
        return;
    }
    s.search_author_profiles.epoch = Arc::new(());
    if let Some(b) = s.search_message_pages.as_mut() {
        if !b.profiles.pages.get(&b.shown).is_some_and(|r| {
            first && !r.attempted.load(Ordering::Acquire) && r.automatic.load(Ordering::Acquire)
        }) {
            b.profiles.waiting = false;
        }
    }
    if let Some(b) = s.search_message_pages.as_ref() {
        for roster in b.profiles.pages.values() {
            if first
                && !roster.attempted.load(Ordering::Acquire)
                && roster.automatic.load(Ordering::Acquire)
            {
                continue;
            }
            roster.automatic.store(false, Ordering::Release);
            for actor in roster.actors.values() {
                actor.retired.store(true, Ordering::Release);
            }
        }
    }
}
pub(crate) fn connection_changed(s: &mut ShellState, team: &str) {
    s.search_author_profiles.applied.remove(team);
    s.search_author_profiles.events.remove(team);
    invalidate(s, team, true);
}
pub(crate) fn invalidate(s: &mut ShellState, team: &str, actors: bool) {
    if !s
        .search_message_pages
        .as_ref()
        .is_some_and(|b| b.frame.team == team)
    {
        return;
    }
    s.search_author_profiles.epoch = Arc::new(());
    if let Some(b) = s.search_message_pages.as_mut() {
        b.profiles.waiting = false;
    }
    if let Some(b) = s.search_message_pages.as_ref() {
        for r in b.profiles.pages.values() {
            r.automatic.store(false, Ordering::Release);
        }
    }
    if actors {
        if let Some(b) = s.search_message_pages.as_ref() {
            for r in b.profiles.pages.values() {
                for a in r.actors.values() {
                    a.retired.store(true, Ordering::Release);
                }
            }
        }
    }
}
pub(crate) fn event(
    s: &mut ShellState,
    team: &str,
    generation: u64,
    e: &super_platinum_core::slack::events::RtEvent,
) {
    if !s
        .core
        .workspaces
        .get(team)
        .is_some_and(|w| w.rt_generation == generation)
    {
        return;
    }
    use super_platinum_core::slack::events::RtEvent;
    if matches!(e,RtEvent::Unknown(raw) if matches!(raw.kind.as_str(),"hello"|"pong"|"presence_change"|"user_typing"))
    {
        return;
    }
    if !matches!(
        e,
        RtEvent::PresenceChange { .. } | RtEvent::UserTyping { .. }
    ) {
        s.search_author_profiles.events.insert(team.into());
    }
    if !s
        .search_message_pages
        .as_ref()
        .is_some_and(|b| b.frame.team == team)
    {
        return;
    }
    match e {
        RtEvent::PreferencesChanged { name, .. } if name == "hidden_users" => {
            invalidate(s, team, false)
        }
        RtEvent::UserChanged(user) => {
            let relevant = s
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| w.self_user_id == user.id)
                || s.search_message_pages.as_ref().is_some_and(|b| {
                    b.profiles
                        .pages
                        .values()
                        .any(|r| r.actors.values().any(|a| a.id == user.id))
                });
            if relevant {
                invalidate(s, team, false);
            }
        }
        RtEvent::MessageChanged { message, .. } | RtEvent::Message(message) => {
            if let (Some(c), Some(ts)) = (&message.channel, &message.ts) {
                retire_actor(s, c, ts);
            }
        }
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => retire_actor(s, channel, deleted_ts),
        RtEvent::Unknown(_) => invalidate(s, team, false),
        _ => {}
    }
}
fn retire_actor(s: &ShellState, c: &str, ts: &str) {
    if let Some(b) = s.search_message_pages.as_ref() {
        for r in b.profiles.pages.values() {
            if let Some(a) = r.actors.get(&(c.into(), ts.into())) {
                a.retired.store(true, Ordering::Release);
            }
        }
    }
}
fn known_denied(s: &ShellState, team: &str, a: &Actor) -> bool {
    let Some(w) = s.core.workspaces.get(team) else {
        return true;
    };
    if w.users.get(&a.id).is_some_and(|u| {
        u.deleted
            || u.is_bot
            || u.is_stranger
            || ["is_app_user", "isUnknown", "isNonExistent"]
                .iter()
                .any(|k| !falsey(u.extra.get(*k)))
    }) {
        return true;
    }
    let bad = |m: &super_platinum_core::slack::models::Message| {
        m.ts.as_deref() == Some(&a.key.1)
            && (m.user.as_ref().is_some_and(|id| id != &a.id)
                || m.bot_id.as_ref().is_some_and(|v| !v.is_empty())
                || m.subtype.as_ref().is_some_and(|v| !v.is_empty())
                || [
                    "isUnknown",
                    "isNonExistent",
                    "isEKMError",
                    "is_ephemeral",
                    "is_deleted",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                ]
                .iter()
                .any(|k| !falsey(m.extra.get(*k))))
    };
    w.messages
        .get(&a.key.0)
        .is_some_and(|b| b.was_deleted(&a.key.1) || b.messages.iter().any(bad))
        || s.core.threads.iter().any(|((t, c, _), b)| {
            t == team && c == &a.key.0 && (b.was_deleted(&a.key.1) || b.messages.iter().any(bad))
        })
}
pub(crate) fn fixture_view(s: &ShellState) -> bool {
    s.core.transport.is_none()
        && s.core.session.is_none()
        && std::env::var("SUPER_PLATINUM_FIXTURE")
            .ok()
            .is_some_and(|v| {
                matches!(
                    v.as_str(),
                    "search-messages-author-profile" | "search-messages-author-profile-narrow"
                )
            })
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.core.transport = None;
    s.core.session = None;
    if let Some(row) = s.search_results.first_mut() {
        row.author = "Avery Chen".into();
    }
    let id = "U_SEARCH_PROFILE_FIXTURE".to_owned();
    if let Some(w) = s
        .core
        .active_team
        .as_ref()
        .and_then(|t| s.core.workspaces.get_mut(t))
    {
        w.users.insert(
            id.clone(),
            super_platinum_core::slack::models::User {
                id: id.clone(),
                name: Some("Search author preview".into()),
                profile: Some(super_platinum_core::slack::models::UserProfile {
                    display_name: Some("Avery Chen".into()),
                    title: Some("Read-only Search author".into()),
                    email: Some("avery@fixture.invalid".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
    }
    s.overlay = None;
    s.profile_user = Some(id.clone());
    s.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
        user: id,
        loading: false,
        error: None,
    });
}
pub(crate) fn retire_ui(s: &mut ShellState) {
    let pending = s
        .search_message_pages
        .as_ref()
        .and_then(|b| b.profiles.prepared.clone())
        .filter(|p| p.pending);
    if let Some(p) = pending {
        fail_pending(s, &p);
    }
}
fn fail_pending(s: &mut ShellState, p: &Arc<Prepared>) {
    if let Some(b) = s.search_message_pages.as_mut() {
        if b.profiles
            .prepared
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, p))
        {
            b.profiles.prepared = Some(Arc::new(Prepared {
                context: p.context.clone(),
                roster: p.roster.clone(),
                members: BTreeMap::new(),
                hidden: BTreeSet::new(),
                pending: false,
                failed: true,
                retry_at: p.retry_at,
                blocked: p.blocked,
            }));
        }
    }
}

// Accepted cache publication changes UI activation, not original raw lineage.
pub(in crate::bootstrap::search) fn cached_activation(s: &mut ShellState) {
    if let Some(b) = s.search_message_pages.as_mut() {
        b.profiles.prepared = None;
        b.profiles.waiting = true;
    }
}

/// Disconnected presentation only; no Actor, Context, Ready or return token.
pub(crate) fn fixture_user(s: &ShellState) -> Option<&super_platinum_core::slack::models::User> {
    if !fixture_view(s) || s.profile_user.as_deref() != Some("U_SEARCH_PROFILE_FIXTURE") {
        return None;
    }
    let user = s
        .core
        .workspaces
        .get(s.core.active_team.as_ref()?)?
        .users
        .get("U_SEARCH_PROFILE_FIXTURE")?;
    (user.name.as_deref() == Some("Search author preview")).then_some(user)
}
