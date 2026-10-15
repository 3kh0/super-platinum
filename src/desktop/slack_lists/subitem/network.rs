use super::*;
use super_platinum_core::{
    slack::{api, client::PreparedRequest},
    slack_lists::PersonalTaskLists,
};
pub(super) struct Loaded {
    pub document: Document,
    pub allow: Option<String>,
    pub personal: PersonalTaskLists,
    pub experiments: api::ListExperiments,
    pub parent: SubitemReadback,
    pub raw: Arc<Value>,
}
fn positive(raw: &Value) -> Result<(), Error> {
    if raw.get("ok") == Some(&Value::Bool(true)) && raw.get("error").is_none_or(Value::is_null) {
        Ok(())
    } else {
        Err(Error::Api("subitem_read_not_positive".into()))
    }
}
fn document(raw: Value, id: &str) -> Result<Document, Error> {
    positive(&raw)?;
    let file = raw
        .get("file")
        .ok_or_else(|| Error::Api("subitem_file_missing".into()))?;
    for k in ["is_deleted", "is_tombstoned"] {
        if file.get(k).is_some_and(|v| v != &Value::Bool(false)) {
            return Err(Error::Api("subitem_file_unavailable".into()));
        }
    }
    for k in ["mode", "filetype"] {
        if file.get(k).is_some_and(|v| v.as_str() != Some("list")) {
            return Err(Error::Api("subitem_file_kind".into()));
        }
    }
    let doc = Document::parse(file.clone()).map_err(|e| Error::Api(e.to_string()))?;
    if doc.id != id {
        return Err(Error::Api("subitem_file_owner".into()));
    }
    Ok(doc)
}
async fn read(
    state: Signal<ShellState>,
    g: &Guard,
    stage: Stage,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !g.current(&state.peek(), stage) || rename::creation_cooldown(&g.scope) {
        return Err(Error::Api("subitem_read_retired".into()));
    }
    let result = if super::super::fixture::enabled(&state.peek()) {
        if !fixture::enabled(&state.peek()) {
            return Err(Error::TransportNotConfigured);
        }
        fixture::read(state, g, stage, request).await
    } else {
        let c = g
            .scope
            .request(&state.peek())
            .ok_or(Error::TransportNotConfigured)?;
        c.transport
            .execute_list_subitem_read(request, || {
                g.current(&state.peek(), stage) && !rename::creation_cooldown(&g.scope)
            })
            .await
            .map_err(|e| match e {
                ListSubitemFailure::NotRequested(e) | ListSubitemFailure::Unconfirmed(e) => e,
            })
    };
    if let Err(e) = &result {
        rename::record_creation_limit(&g.scope, e);
    }
    result
}
async fn load(state: Signal<ShellState>, g: &Guard, stage: Stage) -> Result<Loaded, Error> {
    let (c, w) = if super::super::fixture::enabled(&state.peek()) {
        if !fixture::enabled(&state.peek()) {
            return Err(Error::TransportNotConfigured);
        }
        (state.peek().core.client.clone(), fixture::session())
    } else {
        let c = g
            .scope
            .request(&state.peek())
            .ok_or(Error::TransportNotConfigured)?;
        (c.client, c.session)
    };
    let document = document(
        read(state, g, stage, api::file_info(&c, &w, &g.key.list)).await?,
        &g.key.list,
    )?;
    let raw = read(state, g, stage, api::list_allow_lists(&c, &w)).await?;
    positive(&raw)?;
    let allow = if w.team_id.starts_with('E')
        || w.enterprise_id.as_deref().is_some_and(|s| !s.is_empty())
    {
        api::parse_enterprise_list_allow_lists(&raw)?
    } else {
        api::parse_list_allow_lists(&raw)?
    };
    let raw = read(state, g, stage, api::list_personal_task_preferences(&c, &w)).await?;
    positive(&raw)?;
    let personal = api::parse_list_personal_task_lists(&raw)?;
    let raw = read(state, g, stage, api::list_experiments(&c, &w)).await?;
    positive(&raw)?;
    let experiments = api::parse_list_experiments(&raw)?;
    let raw = read(
        state,
        g,
        stage,
        api::list_record_subtasks(&c, &w, &g.key.list, &g.proof.parent),
    )
    .await?;
    let parent = SubitemReadback::from_response(&g.key.list, &g.proof.parent, &raw)
        .map_err(|e| Error::Api(e.to_string()))?;
    Ok(Loaded {
        document,
        allow,
        personal,
        experiments,
        parent,
        raw: Arc::new(raw),
    })
}
pub(super) fn save(mut state: Signal<ShellState>, g: Guard) {
    dioxus::core::spawn_forever(async move {
        let loaded = match load(state, &g, Stage::Preparing).await {
            Ok(v) => v,
            Err(_) => {
                unsent(
                    &mut state.write(),
                    &g,
                    "Could not verify the parent and permissions. Nothing was sent.",
                );
                return;
            }
        };
        let Some(intent) = with(|r| {
            r.entries
                .get(&g.key)
                .filter(|e| e.serial == g.serial && e.job == g.job)
                .map(|e| e.intent.clone())
        }) else {
            return;
        };
        // Full raw/typed parsing and preflight comparison stays off the renderer.
        let prepared = tokio::task::spawn_blocking(move || prepare_permit(intent, &loaded))
            .await
            .ok()
            .flatten();
        let permit = match prepared.and_then(|p| admit_prepared(&state.peek(), &g, p)) {
            Some(p) => p,
            None => {
                unsent(
                    &mut state.write(),
                    &g,
                    "The parent or List changed. Nothing was sent. Refresh before trying again.",
                );
                return;
            }
        };
        state.write();
        let result = if super::super::fixture::enabled(&state.peek()) {
            fixture::write(state, &g, permit).await
        } else if let Some(c) = g.scope.request(&state.peek()) {
            let timestamp = with(|r| r.entries.get(&g.key).unwrap().timestamp.clone());
            let request = api::list_subitem_create(&c.client, &c.session, permit, &timestamp);
            c.transport
                .execute_list_subitem_create(request, || {
                    g.current(&state.peek(), Stage::Sending) && !rename::creation_cooldown(&g.scope)
                })
                .await
        } else {
            Err(ListSubitemFailure::NotRequested(
                Error::TransportNotConfigured,
            ))
        };
        completed(&mut state.write(), &g, result);
        fixture::finished(&g, "write", false);
    });
}
pub(super) fn check(mut state: Signal<ShellState>, g: Guard) {
    dioxus::core::spawn_forever(async move {
        let result = load(state, &g, Stage::Checking).await;
        let applied = checked(&mut state.write(), &g, result);
        fixture::finished(&g, "check", applied);
    });
}
