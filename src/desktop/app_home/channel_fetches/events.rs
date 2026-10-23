//! Accepted socket partials and source topic invalidation, with original custody.
use super::*;
use super_platinum_core::slack::events::RtEvent;

pub(crate) struct Event {
    owner: Owner,
    observed: Observation,
    native: BTreeMap<String, super_platinum_core::slack::models::Channel>,
    revision: u64,
    patch: Option<Value>,
    topic: Option<topics::Event>,
    facts: Option<Facts>,
    fresh: Result<Option<conditional::continuation::Input>, Error>,
}
pub(crate) struct Work {
    owner: Option<Owner>,
    batch: Option<topics::Batch<u64>>,
    pub(super) startup: Option<(Arc<Context>, bool)>,
}
pub(crate) fn before_event(
    shell: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<Event> {
    let context = Context::capture(shell, team).ok()?;
    if context.rt_generation != generation {
        return None;
    }
    let slot = shell.app_home.channel_fetches.slots.get(&context.key())?;
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    if !owner.publication_current(shell) {
        return None;
    }
    let patch = patch(event, &context.session.user_id);
    let topic = if let RtEvent::Unknown(raw) = event {
        topics::event(raw)
    } else {
        None
    };
    if patch.is_none() && topic.is_none() {
        return None;
    }
    let facts = patch
        .as_ref()
        .and_then(|row| members::receipt(shell, &owner, std::slice::from_ref(row)).ok())
        .map(|r| r.facts);
    let fresh = patch.as_ref().map_or(Ok(None), |row| {
        conditional::source_values(shell, &owner, std::slice::from_ref(row))
    });
    let w = shell.core.workspaces.get(team)?;
    Some(Event {
        observed: observe(shell, &context).ok()?,
        native: w.channels.clone(),
        revision: w.channel_lifecycle.revision(),
        owner,
        patch,
        topic,
        facts,
        fresh,
    })
}
pub(crate) fn after_event(shell: &mut ShellState, event: Option<Event>) -> Option<Work> {
    let event = event?;
    let owner = event.owner;
    if !owner.current(shell) {
        return None;
    }
    let ctx = &owner.context;
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id)?;
    if !event.observed.current(
        &w.directory_channels,
        &ctx.account,
        &ctx.session,
        ctx.epoch,
        GENERATION,
        &event.native,
    ) {
        return None;
    }
    if let Some(row) = event.patch {
        let id = row["id"].as_str()?;
        // The ordinary reducer supplies event timestamp and per-field ordering.
        // An unchanged revision means it rejected this partial; do not enqueue it.
        if w.channel_lifecycle.channel_revision(id) > event.revision {
            let facts = event.facts?;
            let ingress = w
                .directory_channels
                .capture_receipt_ingress(
                    &event.observed,
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    GENERATION,
                    &event.native,
                    Origin::Direct,
                    vec![row],
                    facts.clone(),
                )
                .ok()?;
            let boot = ingress.boot_in_flight();
            w.directory_channels
                .enqueue_ingress(ingress, &ctx.account, &ctx.session, ctx.epoch)
                .ok()?;
            let next = match event
                .fresh
                .and_then(|input| conditional::source_fresh(shell, &owner, input, &facts, boot))
            {
                Ok(next) => next,
                Err(error) => {
                    failed(shell, &owner, error);
                    None
                }
            };
            conditional::continuation::advance(shell, &owner, next).ok()?;
        }
    }
    let serial = shell.app_home.channel_fetches.slots[&ctx.key()].serial;
    let current = shell
        .app_home
        .channel_fetches
        .topic_owners
        .iter()
        .filter(|(_, o)| o.current(shell))
        .map(|(id, _)| *id)
        .collect::<std::collections::BTreeSet<_>>();
    let topic_now = conditional::fixture::topic_now(shell);
    let registry = &mut shell.app_home.channel_fetches;
    registry.topic_owners.retain(|id, _| current.contains(id));
    registry.topics.retain_keys(|id| current.contains(id));
    registry.topic_owners.insert(serial, owner.clone());
    let batch = match event.topic {
        Some(topics::Event::Desync(ids)) => {
            registry.topics.desync(serial, ids);
            None
        }
        Some(topics::Event::Resync(ids)) => registry.topics.push(serial, ids, topic_now),
        None => None,
    };
    Some(Work {
        owner: Some(owner),
        batch,
        startup: None,
    })
}
pub(crate) fn resume(state: Signal<ShellState>, work: Option<Work>) {
    let Some(work) = work else {
        return;
    };
    if let Some(owner) = work.owner {
        drain::kick(state, owner);
    }
    if let Some((context, warm)) = work.startup {
        if context.current(&state.peek()) {
            dioxus::core::spawn_forever(initial::initialize(state, context, warm));
        }
    }
    if let Some(batch) = work.batch {
        publish_topics(state, batch);
    }
    conditional::notifications(state);
    timer(state);
}
fn publish_topics(mut state: Signal<ShellState>, batch: topics::Batch<u64>) {
    let full = {
        let mut shell = state.write();
        let Some(owner) = shell
            .app_home
            .channel_fetches
            .topic_owners
            .get(&batch.key)
            .cloned()
        else {
            return;
        };
        if !owner.publication_current(&shell) {
            return;
        }
        let kind = topics::classify(&batch.topics, &owner.context.session.user_id);
        match kind {
            topics::Invalidation::None => None,
            topics::Invalidation::All => Some(owner.context.session.team_id.clone()),
            topics::Invalidation::Channels(ids) => {
                if let Err(error) = conditional::stale(&mut shell, &owner, &ids) {
                    failed(&mut shell, &owner, error);
                }
                None
            }
            topics::Invalidation::Team => {
                // Team replay requires its timestamp/event-consumer closure. It
                // is separate from ordinary channel conditional acquisition.
                let ids = conditional::fresh_ids(&shell, &owner).unwrap_or_default();
                let _ = conditional::stale(&mut shell, &owner, &ids);
                failed(
                    &mut shell,
                    &owner,
                    Error::Api("channel_team_topic_replay_unavailable".into()),
                );
                None
            }
        }
    };
    if let Some(team) = full {
        initial::resync(state, team);
    }
}
fn timer(mut state: Signal<ShellState>) {
    let token = {
        let mut s = state.write();
        let registry = &mut s.app_home.channel_fetches;
        if registry.topic_timer.is_some() || registry.topics.deadline().is_none() {
            return;
        }
        let token = Arc::new(());
        registry.topic_timer = Some(token.clone());
        token
    };
    dioxus::core::spawn_forever(async move {
        loop {
            let deadline = {
                let s = state.peek();
                let r = &s.app_home.channel_fetches;
                if !r
                    .topic_timer
                    .as_ref()
                    .is_some_and(|t| Arc::ptr_eq(t, &token))
                {
                    return;
                }
                r.topics.deadline()
            };
            let Some(deadline) = deadline else {
                break;
            };
            tokio::time::sleep_until(deadline.into()).await;
            let batch = {
                let mut s = state.write();
                let r = &mut s.app_home.channel_fetches;
                if !r
                    .topic_timer
                    .as_ref()
                    .is_some_and(|t| Arc::ptr_eq(t, &token))
                {
                    return;
                }
                r.topics.due(Instant::now())
            };
            if let Some(batch) = batch {
                publish_topics(state, batch);
            }
        }
        let mut s = state.write();
        let r = &mut s.app_home.channel_fetches;
        if r.topic_timer
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &token))
        {
            r.topic_timer = None;
        }
    });
}
/// Register boot work without blocking history or the realtime connector.
pub(crate) fn request_initialization(shell: &mut ShellState, team: &str, warm: bool) {
    if let Ok(context) = Context::capture(shell, team) {
        shell
            .app_home
            .channel_fetches
            .startup
            .insert(context.key(), (Arc::new(context), warm));
    }
}
/// Socket acceptance changes custody without pretending a full resync occurred.
pub(crate) fn connected(shell: &mut ShellState, team: &str, generation: u64) -> Option<Work> {
    crate::app_home::cfm_lifecycle::connected(shell, team, generation);
    let context = Arc::new(Context::capture(shell, team).ok()?);
    if context.rt_generation != generation {
        return None;
    }
    // The old selected map has already lost its original socket authority.
    // Repaint cached bodies before any early return or replacement read, so a
    // failed reconnect refresh cannot leave the retired artwork on screen.
    shell.emoji_standard_changed(team);
    let key = context.key();
    let startup = shell
        .app_home
        .channel_fetches
        .startup
        .remove(&key)
        .filter(|(old, _)| old.account_current(shell))
        .map(|(_, warm)| (context.clone(), warm));
    let mut work = Work {
        owner: None,
        batch: None,
        startup,
    };
    let Some(old) = shell.app_home.channel_fetches.slots.get(&key) else {
        return Some(work);
    };
    if !old.context.account_current(shell) {
        return Some(work);
    }
    let w = shell.core.workspaces.get(team)?;
    // A generation change must not launder unrepresented native mutations into
    // fresh publication authority over an older accepted queue.
    if !old.publication.current(
        &w.directory_channels,
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
        &w.channels,
    ) {
        return Some(work);
    }
    if old.context.current(shell) {
        work.owner = Some(Owner {
            context: old.context.clone(),
            token: old.token.clone(),
        });
        return Some(work);
    }
    let continuation = conditional::continuation::before_rebind(shell, &context, old);
    let publication = Arc::new(observe(shell, &context).ok()?);
    let slot = shell.app_home.channel_fetches.slots.get_mut(&key)?;
    slot.context = context.clone();
    slot.token = Arc::new(());
    slot.serial = super::super::next();
    slot.publication = publication;
    slot.initial = None;
    slot.drain = None;
    slot.retry = None;
    slot.notify();
    let owner = Owner {
        context,
        token: slot.token.clone(),
    };
    conditional::continuation::rebound(shell, &owner, continuation);
    work.owner = Some(owner);
    Some(work)
}
fn patch(event: &RtEvent, self_id: &str) -> Option<Value> {
    let (id, key, value) = match event {
        RtEvent::Message(m) => {
            let (key, value) = match m.subtype.as_deref()? {
                "channel_name" | "group_name" => ("name", m.extra.get("name")?.clone()),
                "channel_topic" | "group_topic" => {
                    ("topic", serde_json::json!({"value":m.extra.get("topic")?}))
                }
                "channel_purpose" | "group_purpose" => (
                    "purpose",
                    serde_json::json!({"value":m.extra.get("purpose")?}),
                ),
                "channel_archive" | "group_archive" => ("is_archived", true.into()),
                "channel_unarchive" | "group_unarchive" => ("is_archived", false.into()),
                "channel_join" | "group_join" if m.user.as_deref() == Some(self_id) => {
                    ("is_member", true.into())
                }
                "channel_leave" | "group_leave" if m.user.as_deref() == Some(self_id) => {
                    ("is_member", false.into())
                }
                _ => return None,
            };
            (m.channel.as_deref()?, key, value)
        }
        RtEvent::Unknown(raw) => match raw.kind.as_str() {
            "channel_rename" | "group_rename" => (
                raw.rest.get("channel")?.get("id")?.as_str()?,
                "name",
                raw.rest.get("channel")?.get("name")?.clone(),
            ),
            "channel_archive" | "group_archive" => (
                raw.rest.get("channel")?.as_str()?,
                "is_archived",
                true.into(),
            ),
            "channel_unarchive" | "group_unarchive" => (
                raw.rest.get("channel")?.as_str()?,
                "is_archived",
                false.into(),
            ),
            "channel_joined" | "group_joined" => (
                raw.rest.get("channel")?.get("id")?.as_str()?,
                "is_member",
                true.into(),
            ),
            "channel_left" | "group_left" => (
                raw.rest.get("channel")?.as_str()?,
                "is_member",
                false.into(),
            ),
            "member_joined_channel" | "member_left_channel"
                if raw.rest.get("user")?.as_str()? == self_id =>
            {
                (
                    raw.rest.get("channel")?.as_str()?,
                    "is_member",
                    (raw.kind == "member_joined_channel").into(),
                )
            }
            _ => return None,
        },
        _ => return None,
    };
    let mut row = serde_json::Map::new();
    row.insert("id".into(), id.into());
    row.insert(key.into(), value);
    Some(Value::Object(row))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn socket_patch_keeps_only_actual_partial_fields() {
        let event: super_platinum_core::slack::events::RawEvent = serde_json::from_value(serde_json::json!({"type":"channel_rename","channel":{"id":"C1","name":"New"},"event_ts":"200.0"})).unwrap();
        assert_eq!(
            patch(&RtEvent::Unknown(event), "U1"),
            Some(serde_json::json!({"id":"C1","name":"New"}))
        );
    }
}
