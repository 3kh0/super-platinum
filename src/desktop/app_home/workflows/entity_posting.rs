//! Query-owned posting reads. Search rows and default-filled cache models never
//! become channel permission. Keep the resulting witnesses through final Start.
use super::*;
use entity_search::Query;
use super_platinum_core::{
    block_actions::{home::workflow::DirectoryConversation, posting_policy},
    state::posting_metadata::{
        Unknown,
        authority::{Authority, BootRead, RawChannel},
    },
};

#[derive(Clone)]
pub(super) struct Channel {
    proof: Proof,
    custom: bool,
}
#[derive(Clone)]
enum Proof {
    Posting {
        authority: Authority,
        raw: RawChannel,
    },
    // Custom/IM/MPIM do not ask posting policy. Initial local D/G rows retain
    // their exact store object; hydration retains the exact HTTP object instead.
    // Typed model stamps only revoke either observation and never fill wire fields.
    Unrestricted {
        origin: Origin,
        raw: Value,
        stamp: Option<Value>,
    },
}
impl Channel {
    pub(super) fn value(&self) -> &Value {
        match &self.proof {
            Proof::Posting { raw, .. } => raw.value(),
            Proof::Unrestricted { raw, .. } => raw,
        }
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        let Ok(channel) = DirectoryConversation::decode(self.value().clone()) else {
            return false;
        };
        match &self.proof {
            Proof::Posting{authority,raw}=>authority.current(&s.core) && raw.current(&s.core)
                && channel.eligible(self.custom,authority.metadata(&s.core)
                    .and_then(|m|m.effective(authority.configuration(),self.value())).ok()),
            Proof::Unrestricted{origin,stamp,..}=>origin.current(s)
                && channel_stamp(s,origin,channel.id())==*stamp
                && (self.custom || channel.kind()!=super_platinum_core::block_actions::home::workflow::ConversationKind::Channel)
                && channel.eligible(self.custom,None),
        }
    }
}

fn channel_stamp(s: &ShellState, origin: &Origin, id: &str) -> Option<Value> {
    s.core
        .workspaces
        .get(&origin.owner.ctx.session.team_id)
        .and_then(|w| w.channels.get(id))
        .and_then(|c| serde_json::to_value(c).ok())
}
pub(super) async fn unrestricted(
    state: Signal<ShellState>,
    query: &Query,
    id: &str,
) -> Result<Option<Channel>, Error> {
    let origin = query.owner();
    let stamp = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        channel_stamp(&s, origin, id)
    };
    let response = query
        .read(
            state,
            api::conversations_info(
                &origin.owner.ctx.client,
                &origin.owner.ctx.session,
                id.to_owned(),
            ),
        )
        .await?;
    let s = state.peek();
    if !query.current(&s)
        || channel_stamp(&s, origin, id) != stamp
        || response["ok"] != true
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(unavailable());
    }
    let raw = response.get("channel").cloned().ok_or_else(unavailable)?;
    let channel = DirectoryConversation::decode(raw.clone())?;
    if channel.id() != id
        || (!query.custom
            && channel.kind()
                == super_platinum_core::block_actions::home::workflow::ConversationKind::Channel)
    {
        return Err(unavailable());
    }
    Ok(channel.eligible(query.custom, None).then_some(Channel {
        proof: Proof::Unrestricted {
            origin: origin.clone(),
            raw,
            stamp,
        },
        custom: query.custom,
    }))
}

/// hXqI acquires IM/MPIM objects from the actual local store. They do not ask
/// posting/member flags, so initial local suggestions need no per-row network
/// hydration. Start still uses `unrestricted` for its fresh exact-ID read.
pub(super) fn local_direct(s: &ShellState, query: &Query, raw: &Value) -> Result<Channel, Error> {
    let channel = DirectoryConversation::decode(raw.clone())?;
    if !query.current(s)
        || channel.kind()
            == super_platinum_core::block_actions::home::workflow::ConversationKind::Channel
        || channel_stamp(s, query.owner(), channel.id()).as_ref() != Some(raw)
        || !channel.eligible(query.custom, None)
    {
        return Err(unavailable());
    }
    Ok(Channel {
        proof: Proof::Unrestricted {
            origin: query.owner().clone(),
            raw: raw.clone(),
            stamp: Some(raw.clone()),
        },
        custom: query.custom,
    })
}

/// One fresh boot recovery at most. The metadata-only apply preserves unrelated
/// workspace/session/draft state and cannot authenticate a cached BootData.
pub(super) async fn authority(
    state: Signal<ShellState>,
    query: &Query,
) -> Result<Authority, Error> {
    authority_for(state, query, None).await
}

/// A malformed ordered connection ingress can retire routing history without
/// retiring the enclosing boot owner. Recover that missing history once too;
/// a pending/throttled validation is already a known state, not a boot trigger.
pub(super) async fn external_authority(
    state: Signal<ShellState>,
    query: &Query,
    team: &str,
) -> Result<Authority, Error> {
    authority_for(state, query, Some(team)).await
}

async fn authority_for(
    mut state: Signal<ShellState>,
    query: &Query,
    external_team: Option<&str>,
) -> Result<Authority, Error> {
    let ctx = &query.owner().owner.ctx;
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        if let Ok(authority) = Authority::capture(&s.core, &ctx.session.team_id) {
            if external_team.is_none_or(|team| {
                authority.external_team(&s.core, team).is_ok()
                    || authority.external_validation(&s.core).is_ok()
            }) {
                return Ok(authority);
            }
        }
        let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session)
            .map_err(|_| unavailable())?;
        let request = match read.request(&s.core) {
            Ok(request) => request,
            Err(_) => {
                read.failed(&mut s.core);
                return Err(unavailable());
            }
        };
        (read, request)
    };
    let result = query.read(state, request).await;
    let mut s = state.write();
    if !query.current(&s) {
        read.failed(&mut s.core);
        return Err(unavailable());
    }
    let raw = match result {
        Ok(raw) => raw,
        Err(error) => {
            read.failed(&mut s.core);
            return Err(error);
        }
    };
    if !read.apply(&mut s.core, raw, false)? {
        return Err(unavailable());
    }
    let authority = Authority::capture(&s.core, &ctx.session.team_id).map_err(|_| unavailable())?;
    drop(s);
    crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
    Ok(authority)
}

async fn raw_channel(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    id: &str,
) -> Result<RawChannel, Error> {
    let ctx = &query.owner().owner.ctx;
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) || !authority.current(&s.core) {
            return Err(unavailable());
        }
        let read = authority
            .begin_channel_read(&mut s.core, id)
            .map_err(|_| unavailable())?;
        let request = api::conversations_info(&ctx.client, &ctx.session, id.to_owned());
        (read, request)
    };
    let response = query.read(state, request).await?;
    let mut s = state.write();
    if !query.current(&s)
        || !read.current(&s.core)
        || response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(unavailable());
    }
    let raw = response.get("channel").cloned().ok_or_else(unavailable)?;
    let raw = read.apply(&mut s.core, raw).map_err(|_| unavailable())?;
    drop(s);
    crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
    Ok(raw)
}

/// Resolve only a dependency actually requested by the captured policy. No
/// proactive role refreshes and no whole-boot retry loop. The exact role list
/// comes from the shared producer's missing-role query, not membership guesses.
pub(super) async fn load(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    id: &str,
) -> Result<Option<Channel>, Error> {
    // Ordinary listing may reuse an actual current boot/ingress witness. Start
    // performs a fresh exact-ID read; neither route serializes a typed Channel.
    let existing = if query.is_validation() {
        None
    } else {
        authority.raw_channel(&state.peek().core, id).ok()
    };
    let mut raw = match existing {
        Some(raw) => raw,
        None => raw_channel(state, query, authority, id).await?,
    };
    let mut features_read = false;
    let mut roles_read = std::collections::BTreeSet::new();
    let mut changed_role_refresh = false;
    loop {
        let result =
            {
                let mut s = state.write();
                if !query.current(&s) || !authority.current(&s.core) || !raw.current(&s.core) {
                    return Err(unavailable());
                }
                let channel = DirectoryConversation::decode(raw.value().clone())?;
                if query.custom
                || channel.kind()
                    != super_platinum_core::block_actions::home::workflow::ConversationKind::Channel
            {
                return Ok(channel.eligible(query.custom, None).then_some(Channel {
                    proof: Proof::Posting { authority: authority.clone(), raw },
                    custom: query.custom,
                }));
            }
                raw.observe(&mut s.core).and_then(|()| {
                    authority
                        .metadata(&s.core)?
                        .effective(authority.configuration(), raw.value())
                })
            };
        match result {
            Ok(flags) => {
                let channel = DirectoryConversation::decode(raw.value().clone())?;
                return Ok(channel
                    .eligible(query.custom, Some(flags))
                    .then_some(Channel {
                        proof: Proof::Posting {
                            authority: authority.clone(),
                            raw,
                        },
                        custom: query.custom,
                    }));
            }
            Err(Unknown::Policy(posting_policy::Unknown::FeatureAccess)) if !features_read => {
                features_read = true;
                let read = {
                    let mut s = state.write();
                    authority
                        .metadata_mut(&mut s.core)
                        .and_then(|m| m.begin_feature_read(authority.configuration()))
                        .map_err(|_| unavailable())?
                };
                dependency(state, query, authority, read).await?;
            }
            Err(Unknown::Policy(posting_policy::Unknown::ChannelAdmin)) => {
                let missing = {
                    let s = state.peek();
                    authority
                        .metadata(&s.core)
                        .and_then(|m| {
                            m.missing_workspace_roles(authority.configuration(), raw.value())
                        })
                        .map_err(|_| unavailable())?
                };
                if missing.is_empty() || missing.len() > 100 {
                    return Err(unavailable());
                }
                for team in missing {
                    if !roles_read.insert(team.clone()) {
                        return Err(unavailable());
                    }
                    let read = {
                        let mut s = state.write();
                        if !query.current(&s) || !raw.current(&s.core) {
                            return Err(unavailable());
                        }
                        authority
                            .metadata_mut(&mut s.core)
                            .and_then(|m| {
                                m.begin_workspace_role_read(
                                    authority.configuration(),
                                    raw.value(),
                                    &team,
                                )
                            })
                            .map_err(|_| unavailable())?
                    };
                    dependency(state, query, authority, read).await?;
                    if !raw.current(&state.peek().core) {
                        if changed_role_refresh {
                            return Err(unavailable());
                        }
                        changed_role_refresh = true;
                        raw = raw_channel(state, query, authority, id).await?;
                    }
                }
            }
            _ => return Err(unavailable()),
        }
    }
}

async fn dependency(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    read: super_platinum_core::state::posting_metadata::Read,
) -> Result<(), Error> {
    let ctx = &query.owner().owner.ctx;
    let request = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        read.request(
            authority.metadata(&s.core).map_err(|_| unavailable())?,
            authority.configuration(),
            &ctx.client,
            &ctx.session,
        )
        .map_err(|_| unavailable())?
    };
    let response = query.read(state, request).await?;
    let mut s = state.write();
    if !query.current(&s) {
        return Err(unavailable());
    }
    read.apply(
        authority
            .metadata_mut(&mut s.core)
            .map_err(|_| unavailable())?,
        authority.configuration(),
        &response,
    )
    .map_err(|_| unavailable())
}
fn unavailable() -> Error {
    Error::Api("home_workflow_channel_permissions_unavailable".into())
}
