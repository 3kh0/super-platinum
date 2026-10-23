//! Selected standard emoji facts from the original shared initialization read.
//! No picker authority, default version, render-time parsing, or new request.
use super::*;
use super_platinum_core::{
    composer_emoji::standard::Version, slack_lists::comments::edit_seed::prepare::StandardEmoji,
};

pub(super) struct Prepared {
    context: Arc<Context>,
    version: Version,
    raw: Value,
    standard: StandardEmoji,
}

/// This handle belongs to the original assignment read and accepted slot.
/// Keep it across asynchronous work and recheck before using its projection.
#[derive(Clone)]
pub(crate) struct EmojiStandard {
    owner: Owner,
    prepared: Arc<Prepared>,
    serial: u64,
}
impl EmojiStandard {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && self.prepared.context.current(shell)
            && shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.owner.context.key())
                .and_then(|slot| slot.transformations.emoji.as_ref())
                .is_some_and(|actual| Arc::ptr_eq(actual, &self.prepared))
    }
    pub(crate) fn standard(&self) -> &StandardEmoji {
        &self.prepared.standard
    }
    pub(crate) fn version(&self) -> Version {
        self.prepared.version
    }
    pub(crate) fn response(&self) -> &Value {
        &self.prepared.raw
    }
    pub(crate) fn snapshot(&self, shell: &ShellState) -> Value {
        let context = &self.prepared.context;
        serde_json::json!({
            "current":self.current(shell),
            "version":match self.version(){Version::V16=>"v16",Version::V17=>"v17"},
            "response":self.response(),
            "owner":{"account":context.account,"epoch":context.epoch,
                "team":context.session.team_id,"user":context.session.user_id,
                "socket":context.rt_generation,"slotSerial":self.serial}
        })
    }
}

/// Passive capture under the caller's serial shell borrow. A missing or retired
/// original observation is unavailable, not an implicit V16 selection.
pub(crate) fn emoji_standard(shell: &ShellState, team: &str) -> Option<EmojiStandard> {
    let context = Context::capture(shell, team).ok()?;
    let slot = shell.app_home.channel_fetches.slots.get(&context.key())?;
    let captured = EmojiStandard {
        owner: Owner {
            context: slot.context.clone(),
            token: slot.token.clone(),
        },
        prepared: slot.transformations.emoji.clone()?,
        serial: slot.serial,
    };
    captured.current(shell).then_some(captured)
}

// Source KdF+ retains its first module map. Native ownership is narrower: the
// first accepted map in this account/auth/socket context survives resync, but
// replacement context cannot inherit it or relabel the old response as fresh.
pub(super) fn retained(shell: &ShellState, context: &Context) -> Option<Arc<Prepared>> {
    if !context.current(shell) {
        return None;
    }
    emoji_standard(shell, &context.session.team_id).map(|v| v.prepared)
}

pub(super) fn prepared_current(prepared: &Prepared, shell: &ShellState, context: &Context) -> bool {
    prepared.context.key() == context.key() && prepared.context.current(shell)
}

pub(super) async fn prepare(
    state: Signal<ShellState>,
    context: Arc<Context>,
    raw: &Value,
) -> Option<Arc<Prepared>> {
    {
        let shell = state.peek();
        if !context.current(&shell) {
            return None;
        }
        if let Some(prior) = retained(&shell, &context) {
            return Some(prior);
        }
    }
    let raw = raw.clone();
    let (version, raw, standard) = tokio::task::spawn_blocking(move || {
        let version = selected_version(&raw)?;
        let standard = StandardEmoji::build(version).ok()?;
        Some((version, raw, standard))
    })
    .await
    .ok()??;
    if !context.current(&state.peek()) {
        return None;
    }
    Some(Arc::new(Prepared {
        context,
        version,
        raw,
        standard,
    }))
}

fn selected_version(raw: &Value) -> Option<Version> {
    if raw["ok"] != true || raw.get("error").is_some_and(|e| !e.is_null()) {
        return None;
    }
    let assignments = raw.get("assignments")?.as_object()?;
    let group = match assignments.get("emoji_v17") {
        None | Some(Value::Null) => None,
        Some(Value::Object(entry)) => match entry.get("group") {
            None | Some(Value::Null) => None,
            Some(Value::String(group)) => Some(group.as_str()),
            _ => return None,
        },
        _ => return None,
    };
    Some(if group == Some("on") {
        Version::V17
    } else {
        Version::V16
    })
}

#[cfg(test)]
mod tests;

/// Exact closed native adapter for the original factory and accepted slot path.
/// The caller owns synthetic Boot/session setup and an immutable response ledger;
/// this callback can never dispatch through the account transport.
pub(crate) async fn fixture_emoji_assignments<F, Fut>(
    mut state: Signal<ShellState>,
    team: &str,
    read: F,
) -> Result<EmojiStandard, String>
where
    F: FnOnce(super_platinum_core::slack::client::PreparedRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Value, Error>>,
{
    if !fixture_enabled() {
        return Err("Closed shared emoji assignments fixture required".into());
    }
    if crate::canvas::comments::emoji_fixture::requested()
        && !crate::canvas::comments::emoji_fixture::context_enabled(&state.peek())
    {
        return Err("Original disconnected Canvas emoji fixture context required".into());
    }
    if crate::channel_thread_emoji::enabled()
        && !crate::channel_thread_emoji::context_enabled(&state.peek())
    {
        return Err("Original disconnected channel/thread fixture context required".into());
    }
    let context = Arc::new(Context::capture(&state.peek(), team).map_err(|e| e.to_string())?);
    let request =
        super_platinum_core::slack::api::list_experiments(&context.client, &context.session);
    let raw = read(request).await.map_err(|e| e.to_string())?;
    if !context.current(&state.peek()) {
        return Err("Original shared emoji assignments context retired".into());
    }
    let mut transformations = members::Transformations::decode(&raw).map_err(|e| e.to_string())?;
    transformations.emoji = prepare(state, context.clone(), &raw).await;
    let mut shell = state.write();
    let owner = initial::begin(&mut shell, context, Arc::new(transformations), false, false)
        .map_err(|e| e.to_string())?;
    // This exact adapter exercises assignment publication, not fake Init/Extras
    // responses or background channel requests.
    initial::retire(&mut shell, &owner);
    emoji_standard(&shell, team).ok_or_else(|| "Shared emoji observation unavailable".into())
}

fn fixture_enabled() -> bool {
    if crate::canvas::comments::emoji_fixture::requested() {
        return true;
    }
    if crate::channel_thread_emoji::enabled() {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && crate::slack_lists::comments::fixture::compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_EMOJI_COMBINED_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("rich-emoji-combined")
}

/// Executed source-guard probe on a separate shell, never the mounted owner.
/// This is not a claim of native account navigation or a fresh read after RT.
pub(crate) async fn fixture_emoji_guard_probe() -> Result<Value, String> {
    if !fixture_enabled() {
        return Err("Closed shared emoji assignments fixture required".into());
    }
    let standard = tokio::task::spawn_blocking(|| StandardEmoji::build(Version::V17))
        .await
        .map_err(|_| "Isolated emoji map worker failed".to_string())?
        .map_err(|_| "Isolated emoji map unavailable".to_string())?;
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    let team = shell
        .core
        .active_team
        .clone()
        .ok_or("Isolated team missing")?;
    let context = Arc::new(Context::capture(&shell, &team).map_err(|e| e.to_string())?);
    let raw = serde_json::json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}});
    let mut transformations = members::Transformations::decode(&raw).map_err(|e| e.to_string())?;
    transformations.emoji = Some(Arc::new(Prepared {
        context: context.clone(),
        version: Version::V17,
        raw,
        standard,
    }));
    let owner = initial::begin(
        &mut shell,
        context.clone(),
        Arc::new(transformations),
        false,
        false,
    )
    .map_err(|e| e.to_string())?;
    initial::retire(&mut shell, &owner);
    let original = emoji_standard(&shell, &team).ok_or("Isolated observation unavailable")?;
    let before = original.snapshot(&shell);
    let next = context.rt_generation + 1;
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation = next;
    // Exercise the real shared slot rebind as well as original Context refusal.
    // Do not resume returned work: this probe performs no asynchronous IO.
    let _ = events::connected(&mut shell, &team, next);
    let after = original.snapshot(&shell);
    let accessor_available = emoji_standard(&shell, &team).is_some();
    let original_context_current = context.current(&shell);
    if before["current"] != true
        || after["current"] != false
        || accessor_available
        || original_context_current
    {
        return Err("Isolated original emoji source guard did not retire".into());
    }
    Ok(
        serde_json::json!({"scope":"isolated-source-guard", "mountedOwnerMutated":false,
        "transportDisconnected":true,"networkDispatched":false,
        "before":before,"after":after,"socketAfter":next,
        "originalContextCurrent":original_context_current,"accessorAvailable":accessor_available}),
    )
}
