//! rfHj single destination picker. Choosing a user never opens a DM.
use super::*;
use super_platinum_core::slack::models::{Channel, User};
#[derive(Clone)]
pub(super) enum Destination {
    User(User),
    Channel(Channel),
}
impl Destination {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::User(u) => &u.id,
            Self::Channel(c) => &c.id,
        }
    }
    fn choice(&self) -> cal::Choice {
        match self {
            Self::User(u) => cal::Choice {
                id: u.id.clone(),
                name: super_platinum_core::state::display_name(Some(u), &u.id),
            },
            Self::Channel(c) => cal::Choice {
                id: c.id.clone(),
                name: c.name.clone().unwrap_or_else(|| c.id.clone()),
            },
        }
    }
    fn available(&self, s: &ShellState, ctx: &Context) -> bool {
        let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
            return false;
        };
        match self {
            Self::User(u) => {
                !u.deleted
                    && domain::identifier(&u.id)
                    && w.users.get(&u.id).is_none_or(|current| !current.deleted)
            }
            Self::Channel(c) => {
                channel(s, ctx, c) && w.channels.get(&c.id).is_none_or(|c| channel(s, ctx, c))
            }
        }
    }
}
// Ordinary channel policy and freshly observed app metadata are independent.
pub(super) fn channel(s: &ShellState, ctx: &Context, c: &Channel) -> bool {
    if !channel_base(s, ctx, c) || !posting::allowed(s, ctx, c) {
        return false;
    }
    if c.is_im {
        let Some(peer) = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .and_then(|w| w.users.get(c.user.as_ref()?))
        else {
            return false;
        };
        match super_platinum_core::block_actions::app_eligibility::app_dm(peer) {
            Ok(true) => return metadata::allowed(s, ctx, c),
            Ok(false) => {}
            Err(_) => return false,
        }
    }
    true
}
pub(super) fn channel_base(s: &ShellState, ctx: &Context, c: &Channel) -> bool {
    use serde_json::Value;
    if !domain::identifier(&c.id)
        || !(c.is_channel || c.is_group || c.is_private || c.is_im || c.is_mpim)
        || c.is_archived
    {
        return false;
    }
    for key in ["isUnknown", "isNonExistent", "is_frozen", "is_file"] {
        if c.extra.get(key).is_some_and(|v| v != &Value::Bool(false)) {
            return false;
        }
    }
    let props = c.extra.get("properties");
    if props.is_some_and(|v| !v.is_object())
        || props
            .and_then(|v| v.pointer("/huddles/is_huddle_channel"))
            .is_some_and(|v| v != &Value::Bool(false))
    {
        return false;
    }
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return false;
    };
    if c.is_im {
        if c.extra
            .get("is_pending_ext_shared")
            .is_some_and(|v| v != &Value::Bool(false))
        {
            return false;
        }
        let Some(peer) = c.user.as_ref().and_then(|id| w.users.get(id)) else {
            return false;
        };
        if peer.deleted || peer.id == "USLACK" {
            return false;
        }
    }
    true
}
pub(super) fn resolve(s: &ShellState, id: u64) -> Option<String> {
    let c = active(s, id)?;
    let ctx = context(s, id)?;
    let w = s.core.workspaces.get(&ctx.session.team_id)?;
    let cached = w
        .users
        .get(&c.share_channel)
        .is_some_and(|u| !u.deleted && domain::identifier(&u.id))
        || w.channels
            .get(&c.share_channel)
            .is_some_and(|ch| channel(s, &ctx, ch));
    let selected = c
        .share_selected
        .as_ref()
        .is_some_and(|v| v.id() == c.share_channel && v.available(s, &ctx));
    if !cached && !selected {
        return None;
    }
    if c.share_channel.starts_with(['U', 'W']) {
        // Same captured imIdByMemberId lookup, including no mapping => empty.
        Some(
            w.channels
                .values()
                .find(|ch| ch.is_im && ch.user.as_ref() == Some(&c.share_channel))
                .map(|ch| ch.id.clone())
                .unwrap_or_default(),
        )
    } else {
        Some(c.share_channel.clone())
    }
}
pub(super) fn query_current(s: &ShellState, id: u64, generation: u64) -> bool {
    editable(s, id) && active(s, id).is_some_and(|c| c.share_generation == generation)
}
pub(super) fn search(mut state: Signal<ShellState>, id: u64, query: String) {
    let job = {
        let mut s = state.write();
        if !editable(&s, id) || query.len() > 4096 {
            return;
        }
        let ctx = context(&s, id).unwrap();
        active_mut(&mut s, id).unwrap().posting.invalidate();
        let selected = active(&s, id).unwrap().share_channel.clone();
        let choices = load::share_choices(&s, &ctx, &query, &selected);
        let c = active_mut(&mut s, id).unwrap();
        c.share_query = query.clone();
        c.share_choices = choices;
        c.share_remote.clear();
        if let Some(selected) = &c.share_selected {
            if !c.share_choices.iter().any(|v| v.id == selected.id()) {
                c.share_choices.push(selected.choice());
            }
        }
        c.share_generation = c.share_generation.wrapping_add(1);
        c.metadata.invalidate();
        c.share_loading = true;
        (ctx, c.share_generation)
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_millis(180)).await;
        let (ctx, generation) = job;
        let mut remote = Vec::new();
        let mut failed = false;
        // Captured WNgx/53BY/vAgx options from rfHj, with the existing edge
        // discovery observation. No join/open/write is hidden in search.
        for (path, body) in queries(&query) {
            if !query_current(&state.peek(), id, generation) {
                return;
            }
            let ingress =
                crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
            match directory::read(
                state,
                id,
                directory::Query::Sharing(generation),
                &ctx,
                path,
                body,
            )
            .await
            {
                Ok(raw) => {
                    if !query_current(&state.peek(), id, generation) {
                        return;
                    }
                    let mut accepted_channels = Vec::new();
                    let rows = if path == "mpims/search" {
                        raw.get("mpims").or_else(|| raw.get("results"))
                    } else {
                        raw.get("results")
                    };
                    if let Some(rows) = rows
                        .and_then(serde_json::Value::as_array)
                        .filter(|v| v.len() <= 512)
                    {
                        for row in rows.iter().take(100) {
                            let candidate = if path == "users/search" {
                                serde_json::from_value::<User>(row.clone())
                                    .ok()
                                    .map(Destination::User)
                            } else if path == "mpims/search" {
                                super::super::super::inputs::user::directory::decode_mpim(row)
                                    .map(Destination::Channel)
                            } else {
                                serde_json::from_value::<Channel>(row.clone())
                                    .ok()
                                    .map(Destination::Channel)
                            };
                            if let Some(v) = candidate {
                                if let Destination::Channel(channel) = &v {
                                    accepted_channels.push(channel.clone());
                                }
                                if !remote.iter().any(|old: &Destination| old.id() == v.id()) {
                                    remote.push(v);
                                }
                            }
                        }
                    } else {
                        failed = true;
                    }
                    if query_current(&state.peek(), id, generation) {
                        ingress.channels(&state, &accepted_channels);
                    }
                }
                Err(_) => failed = true,
            }
        }
        metadata::refresh(state, id, generation, &ctx, &remote).await;
        if !posting::refresh(state, id, generation, &remote).await {
            failed = true;
        }
        let mut s = state.write();
        if !query_current(&s, id, generation) {
            return;
        }
        remote.retain(|v| v.available(&s, &ctx));
        let selected = active(&s, id).unwrap().share_channel.clone();
        let choices = load::share_choices(&s, &ctx, &query, &selected);
        let c = active_mut(&mut s, id).unwrap();
        c.share_choices = choices;
        if let Some(selected) = &c.share_selected {
            if !c.share_choices.iter().any(|v| v.id == selected.id()) {
                c.share_choices.push(selected.choice());
            }
        }
        c.share_loading = false;
        for v in &remote {
            if !c.share_choices.iter().any(|old| old.id == v.id()) {
                c.share_choices.push(v.choice());
            }
        }
        c.share_remote = remote;
        if failed {
            c.notice = Some(
                "Some sharing destinations could not be verified. Choose an available destination or turn sharing off."
                    .into(),
            );
        }
    });
}

fn queries(query: &str) -> [(&'static str, serde_json::Value); 3] {
    let normalized_mpim =
        super::super::super::inputs::user::directory::normalize_mpim_query(&query);
    [
        (
            "users/search",
            serde_json::json!({"query":query,"count":100,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","search_email":true}),
        ),
        (
            "channels/search",
            serde_json::json!({"query":query,"count":100,"fuzz":1,"filter":"xws","include_record_channels":true}),
        ),
        (
            "mpims/search",
            serde_json::json!({"query":normalized_mpim,"count":100,"fuzz":1,"use_display_name":true}),
        ),
    ]
}
#[cfg(test)]
#[path = "sharing_tests.rs"]
mod tests;
