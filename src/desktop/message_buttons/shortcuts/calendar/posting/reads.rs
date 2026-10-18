use super::*;
use super_platinum_core::state::posting_metadata::Read;

pub(super) async fn authority(
    state: Signal<ShellState>,
    query: &mut Query,
) -> Result<(Authority, bool), Error> {
    let acquired = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        Authority::capture_for_request(&s.core, &query.ctx.transport, &query.ctx.session).ok()
    };
    match acquired {
        Some(a) => Ok((a, false)),
        None => boot(state, query).await.map(|a| (a, true)),
    }
}

/// Exactly one complete metadata-only boot recovery is admitted by refresh.
/// Capture starts before request construction; cached BootData cannot bind it.
pub(super) async fn boot(
    mut state: Signal<ShellState>,
    query: &mut Query,
) -> Result<Authority, Error> {
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        let read = BootRead::begin(&mut s.core, &query.ctx.transport, &query.ctx.session)
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
        Err(e) => {
            read.failed(&mut s.core);
            return Err(e);
        }
    };
    if !read.apply(&mut s.core, raw, false)? {
        return Err(unavailable());
    }
    let authority =
        Authority::capture_for_request(&s.core, &query.ctx.transport, &query.ctx.session)
            .map_err(|_| unavailable())?;
    drop(s);
    crate::bootstrap::persist_workspace_metadata(&state, &query.ctx.session.team_id);
    Ok(authority)
}

async fn raw_channel(
    mut state: Signal<ShellState>,
    query: &mut Query,
    authority: &Authority,
    id: &str,
) -> Result<RawChannel, Unknown> {
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(Unknown::Stale);
        }
        let read = authority.begin_channel_read(&mut s.core, id)?;
        let request = api::conversations_info(&query.ctx.client, &query.ctx.session, id.to_owned());
        (read, request)
    };
    let response = query
        .read(state, request)
        .await
        .map_err(|_| Unknown::Stale)?;
    let mut s = state.write();
    if !query.current(&s) || !read.current(&s.core) {
        return Err(Unknown::Stale);
    }
    if response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Unknown::Shape);
    }
    let raw = response.get("channel").cloned().ok_or(Unknown::Shape)?;
    let raw = read.apply(&mut s.core, raw)?;
    drop(s);
    crate::bootstrap::persist_workspace_metadata(&state, &query.ctx.session.team_id);
    Ok(raw)
}

pub(super) async fn channel(
    mut state: Signal<ShellState>,
    query: &mut Query,
    authority: &Authority,
    id: &str,
) -> Result<Witness, Unknown> {
    let existing = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(Unknown::Stale);
        }
        authority.raw_channel(&s.core, id).ok()
    };
    let mut raw = match existing {
        Some(raw) => raw,
        None => raw_channel(state, query, authority, id).await?,
    };
    let mut features_read = false;
    let mut roles_read = std::collections::BTreeSet::new();
    let mut changed_role_refresh = false;
    loop {
        let result = {
            let mut s = state.write();
            if !query.current(&s) || !raw.current(&s.core) {
                return Err(Unknown::Stale);
            }
            raw.observe(&mut s.core).and_then(|()| {
                authority
                    .metadata(&s.core)?
                    .effective(authority.configuration(), raw.value())
            })
        };
        match result {
            Ok(_) => {
                return Ok(Witness {
                    generation: query.generation,
                    authority: authority.clone(),
                    raw,
                });
            }
            Err(Unknown::Policy(posting_policy::Unknown::FeatureAccess)) if !features_read => {
                features_read = true;
                let read = {
                    let mut s = state.write();
                    if !query.current(&s) {
                        return Err(Unknown::Stale);
                    }
                    authority
                        .metadata_mut(&mut s.core)?
                        .begin_feature_read(authority.configuration())?
                };
                dependency(state, query, authority, read).await?;
            }
            Err(Unknown::Policy(posting_policy::Unknown::ChannelAdmin)) => {
                let missing = {
                    let s = state.peek();
                    authority
                        .metadata(&s.core)?
                        .missing_workspace_roles(authority.configuration(), raw.value())?
                };
                if missing.is_empty() || missing.len() > 100 {
                    return Err(Unknown::Role);
                }
                for team in missing {
                    if !roles_read.insert(team.clone()) {
                        return Err(Unknown::Role);
                    }
                    let read = {
                        let mut s = state.write();
                        if !query.current(&s) || !raw.current(&s.core) {
                            return Err(Unknown::Stale);
                        }
                        authority
                            .metadata_mut(&mut s.core)?
                            .begin_workspace_role_read(
                                authority.configuration(),
                                raw.value(),
                                &team,
                            )?
                    };
                    dependency(state, query, authority, read).await?;
                    if !raw.current(&state.peek().core) {
                        if changed_role_refresh {
                            return Err(Unknown::Stale);
                        }
                        changed_role_refresh = true;
                        raw = raw_channel(state, query, authority, id).await?;
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
}

async fn dependency(
    mut state: Signal<ShellState>,
    query: &mut Query,
    authority: &Authority,
    read: Read,
) -> Result<(), Unknown> {
    let request = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(Unknown::Stale);
        }
        read.request(
            authority.metadata(&s.core)?,
            authority.configuration(),
            &query.ctx.client,
            &query.ctx.session,
        )?
    };
    let response = query
        .read(state, request)
        .await
        .map_err(|_| Unknown::Stale)?;
    let mut s = state.write();
    if !query.current(&s) {
        return Err(Unknown::Stale);
    }
    read.apply(
        authority.metadata_mut(&mut s.core)?,
        authority.configuration(),
        &response,
    )
}
