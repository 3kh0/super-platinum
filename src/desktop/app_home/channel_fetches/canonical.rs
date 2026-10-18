//! Read-only consumer observation of already accepted canonical channel state.
use super::*;
use super_platinum_core::state::directory_channels::Ready;

pub(crate) struct Canonical {
    context: Arc<Context>,
    lifecycle: crate::app_home::cfm_lifecycle::Snapshot,
    ready: Ready,
}
impl Canonical {
    pub(crate) fn current_for(&self, shell: &ShellState, team: &str) -> bool {
        self.context.session.team_id == team && self.current(shell)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.lifecycle.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| {
                    self.ready.current(
                        &w.directory_channels,
                        &self.context.account,
                        &self.context.session,
                        self.context.epoch,
                        GENERATION,
                        &w.channels,
                    )
                })
    }
    pub(crate) fn row(&self, id: &str) -> Option<&Value> {
        self.ready
            .rows()
            .iter()
            .find(|row| row["id"].as_str() == Some(id))
    }
    /// Accepted local source encounter order, never server-wide completeness.
    pub(crate) fn rows(&self) -> &[Value] {
        self.ready.rows()
    }
}
/// A source selector hit returns its current row even when scheduling refuses.
/// Call after releasing the shell borrow; capture itself remains read-only.
pub(crate) fn hit(
    state: Signal<ShellState>,
    observed: &Arc<Canonical>,
    id: &str,
) -> Result<(), Error> {
    if !observed.current(&state.peek()) || observed.row(id).is_none() {
        return Err(unavailable());
    }
    conditional::hit(state, observed.context.clone(), id)
}
/// No request, guessed absence, native-to-source conversion or new freshness
/// admission occurs here. Missing/pending/stale source remains unavailable.
pub(crate) fn capture(shell: &mut ShellState, team: &str) -> Result<Arc<Canonical>, Error> {
    let context = Arc::new(Context::capture(shell, team)?);
    let lifecycle = crate::app_home::cfm_lifecycle::observed(shell, team)?;
    let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let ready = w
        .directory_channels
        .ready(
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
            &w.channels,
        )
        .map_err(|_| unavailable())?;
    Ok(Arc::new(Canonical {
        context,
        lifecycle,
        ready,
    }))
}
/// The ordinary-title branch is deliberately smaller than the generic channel
/// model: file/Canvas/tab, DM, record, huddle and meeting names need other facts.
pub(crate) fn ordinary_label(token: &Value, row: &Value) -> Result<String, Error> {
    if token["type"] != "channel"
        || !ordinary_tab(token)
        || token["channel_id"].as_str().is_none()
        || token["channel_id"] != row["id"]
    {
        return Err(unavailable());
    }
    for key in ["is_im", "is_mpim", "is_file", "isUnknown", "isNonExistent"] {
        if row
            .get(key)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
        {
            return Err(unavailable());
        }
    }
    let properties = row.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object()) {
        return Err(unavailable());
    }
    for (group, field) in [
        ("huddles", "is_huddle_channel"),
        ("meetings_channel", "is_meeting_channel"),
        ("record_channel", "record_id"),
    ] {
        let group = properties
            .and_then(|p| p.get(group))
            .filter(|v| !v.is_null());
        if group.is_some_and(|v| !v.is_object()) {
            return Err(unavailable());
        }
        if group.and_then(|v| v.get(field)).is_some_and(|v| {
            !matches!(v, Value::Null | Value::Bool(false)) && v.as_str() != Some("")
        }) {
            return Err(unavailable());
        }
    }
    let name = row["name"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.starts_with("mc:"))
        .ok_or_else(unavailable)?;
    Ok(format!("#{name}"))
}

/// The source initial_tab branch skips absent/null/empty IDs; preserve those
/// exact raw forms rather than normalizing the serialized token.
pub(crate) fn ordinary_tab(token: &Value) -> bool {
    matches!(token.get("tab_id"), None | Some(Value::Null)) || token["tab_id"].as_str() == Some("")
}

/// Closed fixture only; exercise the actual accepted boot/Init/drain path.
pub(crate) async fn fixture_seed(
    mut state: Signal<ShellState>,
    boot_raw: Value,
    init_raw: Value,
) -> Result<(), String> {
    if !crate::fixture::is_fixture()
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-sections-mentions"
                | "profile-sections-mentions-narrow"
                | "profile-channel-recovery"
                | "profile-channel-recovery-narrow"
                | "profile-member-recovery"
                | "profile-member-recovery-narrow"
                | "profile-member-known"
                | "profile-member-known-narrow"
                | "profile-member-freshness"
                | "profile-member-freshness-narrow"
                | "profile-group-recovery"
                | "profile-group-recovery-narrow")
        )
    {
        return Err("closed mention fixture required".into());
    }
    // Recovery exercises real selector-hit admission, which requires the actual
    // root keypress listener. Await its renderer acknowledgement; never synthesize
    // listener readiness from a fixture flag or the boot Pause transition.
    if (conditional::fixture::enabled()
        || crate::directory_member_fetches::automatic::fixture_enabled())
        && !crate::app_home::cfm_lifecycle::install_listener().await
    {
        return Err("fixture channel keypress listener unavailable".into());
    }
    let owner = fixture_begin(&mut state.write(), boot_raw)?;
    let team = owner.context.session.team_id.clone();
    let pause = crate::app_home::cfm_lifecycle::boot_started(&mut state.write(), &team);
    members::drain(state, &owner)
        .await
        .map_err(|e| format!("fixture members: {e}"))?;
    drain::wait(state, &owner)
        .await
        .map_err(|e| format!("fixture channel drain: {e}"))?;
    let read = {
        let s = state.peek();
        initial::Read {
            owner: owner.clone(),
            token: s.app_home.channel_fetches.slots[&owner.context.key()]
                .initial
                .as_ref()
                .unwrap()
                .token
                .clone(),
            channel_revision: s.core.workspaces[&team].channel_lifecycle.revision(),
        }
    };
    let prepared = payload::prepare(
        state,
        &owner,
        super_platinum_core::state::cfm_payload::PayloadKind::Init,
        &init_raw,
        init_raw.to_string(),
    )
    .await
    .map_err(|e| format!("fixture Init prepare: {e}"))?;
    initial::settle_payload(
        &mut state.write(),
        &read,
        super_platinum_core::state::directory_channels::freshness::refresh::Branch::Init,
        prepared,
    )
    .map_err(|e| format!("fixture Init settlement: {e}"))?;
    payload::kick(state, owner);
    // Unprovided optional branches are explicitly failed, not synthesized.
    for branch in [
        super_platinum_core::state::directory_channels::freshness::refresh::Branch::Channels,
        super_platinum_core::state::directory_channels::freshness::refresh::Branch::Extras,
    ] {
        let mut shell = state.write();
        if shell.app_home.channel_fetches.slots[&read.owner.context.key()]
            .initial
            .is_some()
        {
            initial::branch_failed(&mut shell, &read, branch);
        }
    }
    crate::app_home::cfm_lifecycle::boot_settled(&mut state.write(), pause);
    Ok(())
}
// Shared by the closed native adapter and its pure ownership regression.
fn fixture_begin(shell: &mut ShellState, boot_raw: Value) -> Result<Owner, String> {
    let team = shell
        .core
        .active_team
        .clone()
        .ok_or("fixture team missing")?;
    let before =
        Context::capture(shell, &team).map_err(|e| format!("fixture boot context: {e}"))?;
    let boot = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
        &mut shell.core,
        &before.transport,
        &before.session,
    )
    .map_err(|_| "fixture boot authority unavailable".to_owned())?;
    let boot_mode = crate::directory_member_fetches::automatic::mode::Boot::capture(&boot_raw);
    if !boot
        .apply(&mut shell.core, boot_raw, true)
        .map_err(|e| format!("fixture boot apply: {e}"))?
    {
        return Err("fixture boot refused".into());
    }
    let _ = boot_mode.accepted(shell, &team);
    shell.refresh_from_core();
    // Follow the actual bootstrap registration/accepted-connection adapter. This
    // closed fixture consumes the work directly; resume would start network IO.
    events::request_initialization(shell, &team, true);
    let generation = shell.core.workspaces[&team]
        .rt_generation
        .checked_add(1)
        .ok_or("fixture socket generation exhausted")?;
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation = generation;
    let (context, _) = events::connected(shell, &team, generation)
        .and_then(|work| work.startup)
        .ok_or("fixture accepted connection did not consume registered boot")?;
    let flags = Arc::new(
        members::Transformations::decode(&serde_json::json!({"ok":true,"assignments":{}}))
            .map_err(|e| format!("fixture assignments: {e}"))?,
    );
    initial::begin(shell, context, flags, true, false)
        .map_err(|e| format!("fixture initializer admission: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composed_profile_fixture_keeps_source_owner_and_refuses_foreign_history() {
        fn prepare() -> (ShellState, Value) {
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            crate::profile_edit::sections::fixture::mentions::install(&mut shell);
            let ctx = Context::capture(&shell, shell.core.active_team.as_deref().unwrap()).unwrap();
            let user = serde_json::json!({"id":ctx.session.user_id,"name":"fixture-self","team_id":ctx.session.team_id,"profile":{}});
            let raw = serde_json::json!({"ok":true,"team":{"id":ctx.session.team_id,"name":"Profile mentions"},
                "self":user,"users":[user],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true},
                    {"id":"CMENTION01","name":"project-harbor","is_channel":true,"is_member":true}],"prefs":{}});
            (shell, raw)
        }
        let (mut shell, raw) = prepare();
        let owner = fixture_begin(&mut shell, raw).unwrap();
        assert!(owner.publication_current(&shell));
        assert!(observe(&shell, &owner.context).is_ok());
        assert!(
            shell.core.workspaces[&owner.context.session.team_id]
                .directory_channels
                .has_pending()
        );
        assert!(shell.app_home.channel_fetches.startup.is_empty());
        assert_eq!(
            shell.core.workspaces[&owner.context.session.team_id].rt_generation,
            1
        );
        assert!(
            events::connected(&mut shell, &owner.context.session.team_id, 1)
                .unwrap()
                .startup
                .is_none()
        );
        // An admitted boot/queue is still not a successful split Init.
        assert!(
            crate::app_home::cfm_lifecycle::observed(&mut shell, &owner.context.session.team_id)
                .is_err()
        );
        let (mut foreign, raw) = prepare();
        foreign.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
        assert!(fixture_begin(&mut foreign, raw).is_err());
    }
    #[test]
    fn ordinary_labels_refuse_special_forms_and_preserve_name_bytes() {
        let token = serde_json::json!({"type":"channel","channel_id":"C00000001"});
        let row = serde_json::json!({"id":"C00000001","name":"Cafe\u{301}"});
        assert_eq!(ordinary_label(&token, &row).unwrap(), "#Cafe\u{301}");
        for patch in [
            serde_json::json!({"is_im":true}),
            serde_json::json!({"is_mpim":true}),
            serde_json::json!({"is_file":true}),
            serde_json::json!({"isUnknown":true}),
            serde_json::json!({"name":"mc:calendar"}),
            serde_json::json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
            serde_json::json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        ] {
            let mut special = row.clone();
            special
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(ordinary_label(&token, &special).is_err());
        }
        let mut tab = token;
        for value in [Value::Null, Value::String(String::new())] {
            tab["tab_id"] = value;
            assert!(ordinary_label(&tab, &row).is_ok());
        }
        for value in [Value::Bool(false), Value::String("tab".into())] {
            tab["tab_id"] = value;
            assert!(ordinary_label(&tab, &row).is_err());
        }
    }
}
