//! Source member observations used by channel names; typed users are not facts.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::{
        directory_endpoint::member_info::unknown::PlaceholderAsset,
        workflow_management::member_hydration::{Flags, RendererReply as MemberReply},
    },
    state::directory_members::ReceiptMembers,
};

#[derive(Clone)]
pub(super) struct Transformations {
    pub(super) flags: Flags,
    pub(super) archive: bool,
    pub(super) temp_mute: bool,
    pub(super) emoji: Option<Arc<emoji::Prepared>>,
}
impl Transformations {
    pub(super) fn decode(raw: &Value) -> Result<Self, Error> {
        let assignment = |name: &str| -> Result<bool, Error> {
            match raw["assignments"].get(name) {
                None => Ok(false),
                Some(v) => Ok(v["group"].as_str().ok_or_else(unavailable)? == "on"),
            }
        };
        Ok(Self {
            flags: Flags::decode(raw)?,
            archive: assignment("public_channel_retain_membership_on_archive")?,
            temp_mute: assignment("temp_mute")?,
            emoji: None,
        })
    }
}

/// Actual initialization assignments with their original shared owner. A caller
/// must retain this handle across renderer work, not just a copied Flags value.
pub(crate) struct MemberFlags {
    owner: Owner,
    transformations: Arc<Transformations>,
}
impl MemberFlags {
    pub(crate) fn flags(&self) -> &Flags {
        &self.transformations.flags
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.owner.context.key())
                .is_some_and(|slot| Arc::ptr_eq(&slot.transformations, &self.transformations))
    }
}

/// Call in the same serial read as the consumer's captured-context check. No
/// missing assignment response, warm typed row or foreign slot supplies flags.
pub(crate) fn member_flags(shell: &ShellState, team: &str) -> Result<MemberFlags, Error> {
    let context = Context::capture(shell, team)?;
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get(&context.key())
        .ok_or_else(unavailable)?;
    let observed = MemberFlags {
        owner: Owner {
            context: slot.context.clone(),
            token: slot.token.clone(),
        },
        transformations: slot.transformations.clone(),
    };
    if !observed.current(shell) {
        return Err(unavailable());
    }
    Ok(observed)
}

pub(super) fn peer_ids<'a>(
    rows: impl Iterator<Item = &'a Value>,
    self_id: &str,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for row in rows {
        if truth(&row["is_im"])
            && let Some(id) = row["user"].as_str()
        {
            ids.insert(id.into());
        }
        if truth(&row["is_mpim"]) && !truth(&row["properties"]["has_custom_mpdm_name"]) {
            for id in row["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if id != self_id {
                    ids.insert(id.into());
                }
            }
        }
    }
    ids
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
pub(super) struct Receipt {
    pub(super) facts: Facts,
    pub(super) members: Option<ReceiptMembers>,
}
pub(super) fn receipt(shell: &ShellState, owner: &Owner, rows: &[Value]) -> Result<Receipt, Error> {
    if !owner.publication_current(shell) {
        return Err(unavailable());
    }
    let ctx = &owner.context;
    let observed = observe(shell, ctx)?;
    let targets = rows
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect::<BTreeSet<_>>();
    let peers = peer_ids(
        observed
            .rows()
            .iter()
            .filter(|r| r["id"].as_str().is_some_and(|id| targets.contains(id)))
            .chain(rows),
        &ctx.session.user_id,
    );
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let members = if peers.is_empty() {
        None
    } else {
        let asset = PlaceholderAsset::observed("https://a.slack-edge.com/bv1-13/")?;
        Some(w.directory_members.receipt_members(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &peers,
            &w.users,
            &asset,
        )?)
    };
    let facts = Facts {
        self_id: ctx.session.user_id.clone(),
        current_team: ctx.session.team_id.clone(),
        omit_team: ctx
            .session
            .enterprise_id
            .clone()
            .unwrap_or_else(|| ctx.session.team_id.clone()),
        retain_public_membership_on_archive: shell.app_home.channel_fetches.slots[&ctx.key()]
            .transformations
            .archive,
        members: members
            .as_ref()
            .map(|m| m.rows().clone())
            .unwrap_or_default(),
    };
    Ok(Receipt { facts, members })
}

/// Process the actual raw userBoot/member queue before split channel IO. Never
/// synthesize raw members from a warm typed map to satisfy a missing witness.
pub(super) async fn drain(mut state: Signal<ShellState>, owner: &Owner) -> Result<(), Error> {
    let ctx = &owner.context;
    for batch in 0..64 {
        let (pending, flags) = {
            let s = state.peek();
            if !owner.current(&s) {
                return Err(unavailable());
            }
            let w = &s.core.workspaces[&ctx.session.team_id];
            if !w
                .directory_members
                .has_pending(&ctx.account, &ctx.session)?
            {
                return Ok(());
            }
            let flags = s.app_home.channel_fetches.slots[&ctx.key()]
                .transformations
                .flags
                .clone();
            let generation = format!("channel-initial-members-{}-{batch}", ctx.epoch);
            (
                w.directory_members.prepare(
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    &generation,
                    flags.clone(),
                    &w.users,
                )?,
                flags,
            )
        };
        let generation = pending.generation().to_owned();
        let lowered =
            render::fields_guarded(state, |s| owner.current(s), &generation, pending.fields())
                .await?;
        let reply = MemberReply {
            generation: generation.clone(),
            operation: "ordinary-toLowerCase".into(),
            fields: pending.fields().to_vec(),
            lowered,
        };
        let mut s = state.write();
        if !owner.current(&s) {
            return Err(unavailable());
        }
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        let mut store = w.directory_members.clone();
        let (_, native) = store
            .adopt(
                pending,
                reply,
                &flags,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                &generation,
                &w.users,
            )?
            .into_parts();
        w.directory_members = store;
        w.users = native;
    }
    Err(unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exposed_flags_require_actual_assignment_slot_and_original_owner() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut shell);
        let team = shell.core.active_team.clone().unwrap();
        // A warm source/user map alone cannot invent assignment observations.
        assert!(member_flags(&shell, &team).is_err());
        let raw = json!({"ok":true,"assignments":{
            "search_normalize_visible_external_users":{"group":"on"},
            "preserve_omitted_user_change_member_fields":{"group":"on"}}});
        let transformations = Arc::new(Transformations::decode(&raw).unwrap());
        let context = Arc::new(Context::capture(&shell, &team).unwrap());
        initial::begin(&mut shell, context.clone(), transformations, true, false).unwrap();
        let captured = member_flags(&shell, &team).unwrap();
        assert!(captured.flags() == &Flags::decode(&raw).unwrap());
        assert!(captured.current(&shell));

        shell.core.account_epoch += 1;
        assert!(!captured.current(&shell));
        assert!(member_flags(&shell, &team).is_err());
        shell.core.account_epoch -= 1;
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
        assert!(!captured.current(&shell));
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation -= 1;

        let old_token = shell.core.session.as_ref().unwrap().workspaces[&team]
            .token
            .clone();
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .token = "replacement-fixture-token".into();
        assert!(!captured.current(&shell));
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .token = old_token;
        assert!(captured.current(&shell));

        // A replacement read is distinct even when the returned flag values
        // happen to be equal. The old renderer lease cannot adopt with it.
        shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&context.key())
            .unwrap()
            .transformations = Arc::new(Transformations::decode(&raw).unwrap());
        assert!(!captured.current(&shell));
        assert!(member_flags(&shell, &team).unwrap().current(&shell));
    }
}
