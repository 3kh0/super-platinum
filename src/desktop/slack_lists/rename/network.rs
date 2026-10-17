use super::*;
use super_platinum_core::{
    slack::{
        api,
        client::PreparedRequest,
        transport::{ListColumnRenameFailure, ListColumnRenameReadFailure},
    },
    slack_lists::PersonalTaskLists,
};
pub(super) struct Metadata {
    pub document: Document,
    pub allow: Option<String>,
    pub personal: PersonalTaskLists,
    pub experiments: Option<super_platinum_core::slack::api::ListExperiments>,
}
pub(super) fn positive(value: &Value) -> Result<(), Error> {
    if value.get("ok") != Some(&Value::Bool(true))
        || value.get("error").is_some_and(|e| !e.is_null())
    {
        Err(Error::Api("list_rename_read_not_positive".into()))
    } else {
        Ok(())
    }
}
pub(super) fn document(value: Value, id: &str) -> Result<Document, Error> {
    positive(&value)?;
    let raw = value
        .get("file")
        .ok_or_else(|| Error::Api("list_rename_file_missing".into()))?;
    for marker in ["is_deleted", "is_tombstoned"] {
        if raw.get(marker).is_some_and(|v| v != &Value::Bool(false)) {
            return Err(Error::Api("list_rename_file_unavailable".into()));
        }
    }
    for marker in ["mode", "filetype"] {
        if raw.get(marker).is_some_and(|v| v.as_str() != Some("list")) {
            return Err(Error::Api("list_rename_file_kind".into()));
        }
    }
    let parsed = Document::parse(raw.clone()).map_err(|e| Error::Api(e.to_string()))?;
    if parsed.id != id {
        return Err(Error::Api("list_rename_file_owner".into()));
    }
    Ok(parsed)
}
pub(super) fn same_incarnation(a: &Document, b: &Document) -> bool {
    a.id == b.id
        && ["created", "user"]
            .iter()
            .all(|k| a.raw.get(*k) == b.raw.get(*k))
}
fn is_deletion(guard: &Guard) -> bool {
    with_registry(|r| {
        r.entries
            .get(&guard.key)
            .is_some_and(|e| e.serial == guard.serial && e.target.is_deletion())
    })
}
async fn read(
    state: Signal<ShellState>,
    guard: &Guard,
    stage: Stage,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !guard.current(&state.peek(), stage) || registry::blocked(&guard.key.owner) {
        return Err(Error::Api("rename_read_retired".into()));
    }
    let result = if super::super::fixture::enabled(&state.peek()) {
        fixture::read(state, guard, stage, request).await
    } else {
        let context = guard
            .scope
            .request(&state.peek())
            .ok_or(Error::TransportNotConfigured)?;
        if is_deletion(guard) {
            context
                .transport
                .execute_list_column_delete_read(request, || {
                    guard.current(&state.peek(), stage) && !registry::blocked(&guard.key.owner)
                })
                .await
                .map_err(|e| match e {
                    ListColumnRenameFailure::NotRequested(e)
                    | ListColumnRenameFailure::Unconfirmed(e) => e,
                })
        } else {
            context
                .transport
                .execute_list_column_rename_read(request, || {
                    guard.current(&state.peek(), stage) && !registry::blocked(&guard.key.owner)
                })
                .await
                .map_err(|e| match e {
                    ListColumnRenameReadFailure::NotRequested(e)
                    | ListColumnRenameReadFailure::Unconfirmed(e) => e,
                })
        }
    };
    if let Err(e) = &result {
        registry::record(&guard.key.owner, e);
    }
    // Keep the response for the original receipt. Caller owns projection admission.
    result
}
async fn metadata(
    state: Signal<ShellState>,
    guard: &Guard,
    stage: Stage,
) -> Result<Metadata, Error> {
    let (client, session) = if super::super::fixture::enabled(&state.peek()) {
        if !fixture::enabled(&state.peek()) {
            return Err(Error::TransportNotConfigured);
        }
        (state.peek().core.client.clone(), fixture::session())
    } else {
        let c = guard
            .scope
            .request(&state.peek())
            .ok_or(Error::TransportNotConfigured)?;
        (c.client, c.session)
    };
    let raw = read(
        state,
        guard,
        stage,
        api::file_info(&client, &session, &guard.key.list),
    )
    .await?;
    let document = document(raw, &guard.key.list)?;
    let raw = read(
        state,
        guard,
        stage,
        api::list_allow_lists(&client, &session),
    )
    .await?;
    positive(&raw)?;
    let allow = if session.team_id.starts_with('E')
        || session
            .enterprise_id
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        api::parse_enterprise_list_allow_lists(&raw)?
    } else {
        api::parse_list_allow_lists(&raw)?
    };
    let raw = read(
        state,
        guard,
        stage,
        api::list_personal_task_preferences(&client, &session),
    )
    .await?;
    positive(&raw)?;
    let personal = api::parse_list_personal_task_lists(&raw)?;
    let experiments = if is_deletion(guard) {
        let raw = read(
            state,
            guard,
            stage,
            api::list_experiments(&client, &session),
        )
        .await?;
        Some(api::parse_list_experiments(&raw)?)
    } else {
        None
    };
    Ok(Metadata {
        experiments,
        document,
        allow,
        personal,
    })
}
pub(super) fn save(mut state: Signal<ShellState>, guard: Guard, intent: Intent) {
    dioxus::core::spawn_forever(async move {
        let loaded = match metadata(state, &guard, Stage::Preparing).await {
            Ok(v) => v,
            Err(_) => {
                unsent(
                    &mut state.write(),
                    &guard,
                    "Could not verify the schema and permissions. Nothing was sent. Close this editor and refresh before trying again.",
                );
                return;
            }
        };
        let permit = admitted(&state.peek(), &guard, &intent, &loaded);
        let Some(permit) = permit else {
            unsent(
                &mut state.write(),
                &guard,
                "This List changed before the request. Nothing was sent. Close this editor and refresh.",
            );
            return;
        };
        state.write();
        let result = if super::super::fixture::enabled(&state.peek()) {
            fixture::write(state, &guard, permit).await
        } else if let Some(context) = guard.scope.request(&state.peek()) {
            let deletion = permit.is_deletion();
            let text_creation = permit.is_text_creation();
            let sort = permit.is_sort();
            let grouping = permit.is_grouping();
            let filter = permit.is_filter();
            let visibility = permit.is_visibility();
            let precision = permit.is_precision();
            let creation = permit.is_creation();
            let view = permit.is_view();
            let request = permit.request(&context.client, &context.session);
            let current = || {
                guard.current(&state.peek(), Stage::Saving) && !registry::blocked(&guard.key.owner)
            };
            if deletion {
                context
                    .transport
                    .execute_list_column_delete(request, current)
                    .await
            } else if text_creation {
                context
                    .transport
                    .execute_list_column_create(request, current)
                    .await
            } else if filter {
                context
                    .transport
                    .execute_list_view_filter(request, current)
                    .await
            } else if grouping {
                context
                    .transport
                    .execute_list_view_grouping(request, current)
                    .await
            } else if sort {
                context
                    .transport
                    .execute_list_view_sort(request, current)
                    .await
            } else if visibility {
                context
                    .transport
                    .execute_list_field_visibility(request, current)
                    .await
            } else if precision {
                context
                    .transport
                    .execute_list_column_precision(request, current)
                    .await
            } else if creation {
                context
                    .transport
                    .execute_list_view_create(request, current)
                    .await
            } else if view {
                context
                    .transport
                    .execute_list_view_rename(request, current)
                    .await
            } else {
                context
                    .transport
                    .execute_list_column_rename(request, current)
                    .await
            }
        } else {
            Err(ListColumnRenameFailure::NotRequested(
                Error::TransportNotConfigured,
            ))
        };
        completed(&mut state.write(), &guard, result);
        fixture::finished(&guard, "write", false);
    });
}
pub(super) fn check(mut state: Signal<ShellState>, guard: Guard) {
    dioxus::core::spawn_forever(async move {
        let result = metadata(state, &guard, Stage::Checking).await;
        let applied = checked(&mut state.write(), &guard, result);
        fixture::finished(&guard, "check", applied);
    });
}
