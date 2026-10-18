//! Result identity and retained evidence. Local model stamps only invalidate;
//! they never fill omitted directory or permission fields.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::block_actions::{
    app_eligibility::agents::Access,
    home::workflow::{Directory, DirectoryConversation, DirectoryUser},
};
mod ranking;
mod ranking_snapshot;
#[cfg(test)]
mod tests;

#[derive(Clone)]
enum Entity {
    User {
        user: DirectoryUser,
        agents: Option<Access>,
        cache: Option<Value>,
    },
    Conversation {
        raw: Value,
        checked: entity_posting::Channel,
    },
}

#[derive(Clone)]
pub(super) struct Choice {
    query: entity_search::Query,
    scope: entity_external::Scope,
    label: String,
    entity: Entity,
    // Exact pre-sort metadata, retained separately from selection authority.
    matching: Option<entity_members::MatchInfo>,
    group: &'static str,
    previous: Option<entity_channels::Previous>,
    mpim: Option<entity_local_conversations::MpimMetadata>,
    local: Option<entity_local_conversations::Witness>,
    channel: Option<entity_channels::Witness>,
}
impl Choice {
    pub(super) fn label(&self) -> &str {
        &self.label
    }
    pub(super) async fn select(&self, state: Signal<ShellState>) -> Result<Selection, Error> {
        if self
            .channel
            .as_ref()
            .is_some_and(|w| !w.current(&state.peek()))
            || !self.query.current(&state.peek())
            || !self.scope.current(&state.peek())
            || self
                .local
                .as_ref()
                .is_some_and(|w| !w.current(&state.peek()))
        {
            return Err(unavailable());
        }
        let proof = match &self.entity {
            Entity::User {
                user,
                agents,
                cache,
            } => {
                if !user.eligible(agents.as_ref())
                    || !cached_user_eligible(cache, agents.as_ref())
                    || cache_user(&state.peek(), self.query.owner(), &user.user().id) != *cache
                {
                    return Err(unavailable());
                }
                Proof::User {
                    user: user.clone(),
                    agents: agents.clone(),
                    cache: cache.clone(),
                }
            }
            Entity::Conversation { raw, checked } => {
                // Identity must survive hydration. The result does not authorize
                // opening a replacement DM or substituting a different channel.
                if checked.value()["id"] != raw["id"] || !checked.current(&state.peek()) {
                    return Err(unavailable());
                }
                Proof::Conversation(checked.clone())
            }
        };
        if self
            .channel
            .as_ref()
            .is_some_and(|w| !w.current(&state.peek()))
            || !self.query.current(&state.peek())
            || !self.scope.current(&state.peek())
            || self
                .local
                .as_ref()
                .is_some_and(|w| !w.current(&state.peek()))
        {
            return Err(unavailable());
        }
        Ok(Selection {
            origin: self.query.owner().clone(),
            scope: Some(self.scope.clone()),
            local: self.local.clone(),
            channel: self.channel.clone(),
            field: self.query.field.clone(),
            custom: self.query.custom,
            query: self.query.text.clone(),
            label: self.label.clone(),
            proof,
        })
    }
}

#[derive(Clone)]
enum Proof {
    User {
        user: DirectoryUser,
        agents: Option<Access>,
        cache: Option<Value>,
    },
    Conversation(entity_posting::Channel),
}
#[derive(Clone)]
pub(in crate::app_home::workflows) struct Selection {
    origin: Origin,
    scope: Option<entity_external::Scope>,
    local: Option<entity_local_conversations::Witness>,
    channel: Option<entity_channels::Witness>,
    field: String,
    custom: bool,
    query: String,
    label: String,
    proof: Proof,
}
impl Selection {
    pub(super) fn id(&self) -> &str {
        match &self.proof {
            Proof::User { user, .. } => &user.user().id,
            Proof::Conversation(c) => c.value()["id"].as_str().unwrap(),
        }
    }
    pub(super) fn label(&self) -> &str {
        &self.label
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && self.scope.as_ref().is_none_or(|scope| scope.current(s))
            && self.local.as_ref().is_none_or(|w| w.current(s))
            && self.channel.as_ref().is_none_or(|w| w.current(s))
            && match &self.proof {
                Proof::User {
                    user,
                    agents,
                    cache,
                } => {
                    user.eligible(agents.as_ref())
                        && cached_user_eligible(cache, agents.as_ref())
                        && cache_user(s, &self.origin, &user.user().id) == *cache
                }
                Proof::Conversation(channel) => channel.current(s),
            }
    }
    pub(super) fn matches(&self, review: &Preflight, custom: bool, field: &str) -> bool {
        self.custom == custom
            && self.field == field
            && review.value(field, custom) == Some(self.id())
    }
    pub(super) fn query_text(&self) -> &str {
        &self.query
    }
    pub(super) async fn refresh(
        &self,
        state: Signal<ShellState>,
        dialog: u64,
        token: u64,
        review: &Preflight,
    ) -> Result<Self, Error> {
        let query = entity_search::Query::validation(
            state,
            dialog,
            token,
            review,
            self.field.clone(),
            self.custom,
            self.query.clone(),
        )
        .ok_or_else(unavailable)?;
        let choices = load(state, &query).await?;
        let selected = choices
            .into_iter()
            .find(|choice| match &choice.entity {
                Entity::User { user, .. } => user.user().id == self.id(),
                Entity::Conversation { raw, .. } => raw["id"].as_str() == Some(self.id()),
            })
            .ok_or_else(unavailable)?;
        selected.select(state).await
    }
}

pub(super) async fn load(
    state: Signal<ShellState>,
    query: &entity_search::Query,
) -> Result<Vec<Choice>, Error> {
    let names = entity_names::capture(state, query).await?;
    let scope = entity_external::capture(state, query).await?;
    let pages = query.load(state, &scope).await?;
    let mut users = Vec::new();
    for group in &pages {
        if group.directory() == Directory::Users {
            for raw in group.rows() {
                users.push(DirectoryUser::decode(raw)?);
            }
        }
    }
    let users = entity_members::matching(state, query, users).await?;
    let agent_users: Vec<_> = users.iter().map(|u| u.user.clone()).collect();
    let agents = entity_agents::load(state, query, &agent_users).await?;
    let mut choices = Vec::new();
    let mut seen = BTreeSet::new();
    let s = state.peek();
    if !query.current(&s) {
        return Err(unavailable());
    }
    for member in users {
        let user = member.user;
        let cache = member.cache;
        if !user.eligible(agents.as_ref())
            || !cached_user_eligible(&cache, agents.as_ref())
            || cache_user(&s, query.owner(), &user.user().id) != cache
            || !seen.insert(user.user().id.clone())
        {
            continue;
        }
        let label = names
            .policy
            .member_name(user.raw(), "Name hidden")
            .map_err(|_| unavailable())?;
        choices.push(Choice {
            query: query.clone(),
            scope: scope.clone(),
            label,
            matching: Some(member.matching),
            group: "member",
            mpim: None,
            local: None,
            channel: None,
            previous: None,
            entity: Entity::User {
                user,
                agents: agents.clone(),
                cache,
            },
        });
    }
    drop(s);
    for local in entity_local_conversations::load(state, query, &scope, &names).await? {
        let id = local.raw["id"].as_str().ok_or_else(unavailable)?;
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let checked = if query.is_validation() {
            let Some(checked) = entity_posting::unrestricted(state, query, id).await? else {
                continue;
            };
            checked
        } else {
            entity_posting::local_direct(&state.peek(), query, &local.raw)?
        };
        if !local.witness.current(&state.peek()) || !scope.allows(checked.value()) {
            continue;
        }
        let group = if local.raw["is_im"] == true {
            "ims"
        } else {
            "mpim"
        };
        choices.push(Choice {
            query: query.clone(),
            scope: scope.clone(),
            label: local.label,
            matching: Some(local.matching),
            group,
            mpim: local.mpim,
            local: Some(local.witness),
            channel: None,
            previous: None,
            entity: Entity::Conversation {
                raw: local.raw,
                checked,
            },
        });
    }
    let mut authority = None;
    for candidate in entity_channels::load(state, query, &pages).await? {
        let id = candidate.raw["id"].as_str().ok_or_else(unavailable)?;
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let checked = if query.custom {
            entity_posting::unrestricted(state, query, id).await?
        } else {
            if authority.is_none() {
                authority = Some(entity_posting::authority(state, query).await?);
            }
            entity_posting::load(state, query, authority.as_ref().unwrap(), id).await?
        };
        let Some(checked) = checked else { continue };
        if !candidate.witness.current(&state.peek()) || !scope.allows(checked.value()) {
            continue;
        }
        choices.push(Choice {
            query: query.clone(),
            scope: scope.clone(),
            label: candidate.label,
            matching: Some(candidate.matching),
            group: "channel",
            previous: Some(candidate.previous),
            mpim: None,
            local: None,
            channel: Some(candidate.witness),
            entity: Entity::Conversation {
                raw: candidate.raw,
                checked,
            },
        });
    }
    for group in &pages {
        if group.directory() == Directory::Users {
            continue;
        }
        for (index, row) in group.rows().iter().enumerate() {
            // MPIM identity needs actual details/current raw cache; absence is a
            // retained unavailable result, never a synthetic G destination.
            let raw = match group.conversation(index, None) {
                Ok(raw) => raw,
                Err(_) => continue,
            };
            let channel = match DirectoryConversation::decode(raw.clone()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Ordinary channels have already been matched against local plus
            // returned rows with actual dfPN names and previous-name evidence.
            if channel.kind()
                == super_platinum_core::block_actions::home::workflow::ConversationKind::Channel
            {
                continue;
            }
            if !seen.insert(channel.id().to_owned()) {
                continue;
            }
            let checked = if query.custom
                || channel.kind()
                    != super_platinum_core::block_actions::home::workflow::ConversationKind::Channel
            {
                entity_posting::unrestricted(state, query, channel.id()).await?
            } else {
                if authority.is_none() {
                    authority = Some(entity_posting::authority(state, query).await?);
                }
                entity_posting::load(state, query, authority.as_ref().unwrap(), channel.id())
                    .await?
            };
            let Some(checked) = checked else {
                continue;
            };
            if !scope.allows(checked.value()) {
                continue;
            }
            let label = row
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    raw.get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or(channel.id())
                .to_owned();
            choices.push(Choice {
                query: query.clone(),
                scope: scope.clone(),
                label,
                matching: None,
                group: "channel",
                mpim: None,
                local: None,
                channel: None,
                previous: None,
                entity: Entity::Conversation { raw, checked },
            });
        }
    }
    if !query.current(&state.peek())
        || !scope.current(&state.peek())
        || !names.current(&state.peek())
    {
        return Err(unavailable());
    }
    if query.is_validation() {
        // Final admission searches every acquired eligible identity. The
        // display shortlist must never erase a retained selected destination.
        return Ok(choices);
    }
    let observed_rows = choices.clone();
    let (input, witness) = ranking_snapshot::capture(state, query, &choices, &names).await?;
    let ranked = ranking::order(state, query, choices, input).await?;
    if !witness.current(&state.peek(), query, &observed_rows) {
        return Err(unavailable());
    }
    Ok(ranked)
}

fn cache_user(s: &ShellState, origin: &Origin, id: &str) -> Option<Value> {
    s.core
        .workspaces
        .get(&origin.owner.ctx.session.team_id)
        .and_then(|w| w.users.get(id))
        .and_then(|u| serde_json::to_value(u).ok())
}
fn cached_user_eligible(cache: &Option<Value>, agents: Option<&Access>) -> bool {
    // A known newer local revocation can refuse a remote row. It never grants
    // selection: the actual directory row still has to pass independently.
    cache
        .as_ref()
        .is_none_or(|raw| DirectoryUser::decode(raw).is_ok_and(|user| user.eligible(agents)))
}
fn user_label(raw: &Value) -> String {
    [
        raw.pointer("/profile/display_name"),
        raw.pointer("/profile/real_name"),
        raw.get("real_name"),
        raw.get("name"),
        raw.get("id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .find(|s| !s.is_empty())
    .unwrap_or("Unavailable person")
    .to_owned()
}
fn unavailable() -> Error {
    Error::Api("home_workflow_entity_unavailable".into())
}
