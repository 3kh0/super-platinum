use super::*;
use super_platinum_core::slack::{client::PreparedRequest, transport::ListAssigneeFailure};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Access,
    Share,
}
type WorkResult<T> = Result<T, &'static str>;
mod io;
use io::Context;

pub(super) fn start(mut state: Signal<ShellState>, guard: Guard, action: Action) {
    if !guard.current(&state.peek()) {
        return;
    }
    let Some(context) = io::capture(&state.peek(), &guard) else {
        guard.update(|e| {
            e.stage = Stage::Blocked;
            e.message = "Sign in again to check this assignment.".into();
        });
        let _ = state.write();
        return;
    };
    guard.update(|e| {
        e.job = Some(guard.job);
        e.stage = if action == Action::Share {
            Stage::Sharing
        } else {
            Stage::Checking
        };
        e.message = if action == Action::Share {
            "Checking sharing permissions…"
        } else {
            "Checking the selected people’s access…"
        }
        .into();
    });
    let _ = state.write();
    dioxus::core::spawn_forever(async move {
        let result = run(state, &guard, &context, action).await;
        match result {
            Ok(Some(attempt)) => network::dispatch(state, attempt),
            Ok(None) => guard.update(|e| e.job = None),
            Err(message) => guard.update(|e| {
                e.job = None;
                e.stage = if e.recipients.iter().any(|p| {
                    p.dm_attempted
                        || matches!(
                            p.result,
                            ShareResult::Sending
                                | ShareResult::Acknowledged
                                | ShareResult::Uncertain
                        )
                }) {
                    Stage::Uncertain
                } else {
                    Stage::Blocked
                };
                e.message = message.into();
            }),
        }
        let _ = state.write();
    });
}
async fn read(
    state: Signal<ShellState>,
    guard: &Guard,
    context: &Context,
    request: PreparedRequest,
) -> WorkResult<Value> {
    if !guard.current(&state.peek()) {
        return Err("The editor was replaced. No further request was sent.");
    }
    let result = io::execute(state, guard, context, request).await;
    if let Err(ListAssigneeFailure::NotRequested(error) | ListAssigneeFailure::Unconfirmed(error)) =
        &result
    {
        guard.update(|e| cool_down(e, error));
        record_cooldown(&guard.key, error);
    }
    if !guard.current(&state.peek()) {
        return Err(
            "This check belongs to an earlier editor. Reopen the assignment to check its receipt.",
        );
    }
    result.map_err(|_|"Could not check this assignment. No assignment was sent; use Check access to read again.")
}
fn enterprise(context: &Context) -> bool {
    context.session.team_id.starts_with('E')
        || context
            .session
            .enterprise_id
            .as_deref()
            .is_some_and(|s| !s.is_empty())
}
async fn fresh(state: Signal<ShellState>, guard: &Guard, c: &Context) -> WorkResult<()> {
    let id = &guard.key.list;
    let raw = read(state, guard, c, api::file_info(&c.client, &c.session, id)).await?;
    api::assignee_positive(&raw).map_err(|_| "The current List could not be verified.")?;
    let document = Document::parse(
        raw.get("file")
            .cloned()
            .ok_or("The current List is missing.")?,
    )
    .map_err(|_| "The current List is malformed.")?;
    let records = read(
        state,
        guard,
        c,
        api::lists_records_list(&c.client, &c.session, id),
    )
    .await?;
    let records = api::parse_list_records(records, id)
        .map_err(|_| "The loaded List page could not be verified.")?;
    let allow = read(
        state,
        guard,
        c,
        api::list_allow_lists(&c.client, &c.session),
    )
    .await?;
    api::assignee_positive(&allow)
        .map_err(|_| "Your current List access could not be verified.")?;
    let allow = if enterprise(c) {
        api::parse_enterprise_list_allow_lists(&allow)
    } else {
        api::parse_list_allow_lists(&allow)
    }
    .map_err(|_| "Your current List access could not be verified.")?;
    let personal = read(
        state,
        guard,
        c,
        api::list_personal_task_preferences(&c.client, &c.session),
    )
    .await?;
    let personal = api::parse_list_personal_task_lists(&personal)
        .map_err(|_| "Your current task preferences could not be verified.")?;
    let e = guard.entry().ok_or("This assignment was replaced.")?;
    if records != *e.records {
        return Err(
            "The loaded page changed. No assignment was sent; the share receipt is retained.",
        );
    }
    e.intent.revalidate(&document,&records,allow.as_deref(),&personal).map_err(|_|"The List or your access changed. No assignment was sent; the share receipt is retained.")
}
async fn access(state: Signal<ShellState>, guard: &Guard, c: &Context) -> WorkResult<()> {
    let e = guard.entry().ok_or("This assignment was replaced.")?;
    let pending: Vec<_> = e.intent.users_to_check().map(str::to_owned).collect();
    for id in pending {
        let value = read(
            state,
            guard,
            c,
            api::list_assignee_access(&c.client, &c.session, &guard.key.list, &id),
        )
        .await?;
        let mut result = Ok(());
        guard.update(|e| result = e.intent.record_access(&id, &value));
        result.map_err(|_|"This person’s access could not be verified. Use Check access to try the read again.")?;
        if value.get("can_see") == Some(&Value::Bool(true)) {
            observe_previous_share(&guard.key, &id);
        }
    }
    let missing = guard
        .entry()
        .unwrap()
        .intent
        .sharing_needed()
        .map_err(|_| "Wait for the access checks.")?;
    for id in missing {
        let already = guard.entry().unwrap().recipients.iter().any(|p| p.id == id);
        if already {
            let value = read(
                state,
                guard,
                c,
                api::list_assignee_access(&c.client, &c.session, &guard.key.list, &id),
            )
            .await?;
            api::assignee_positive(&value)
                .map_err(|_| "This person’s access could not be verified.")?;
            match value.get("can_see").and_then(Value::as_bool) {
                Some(true) => {
                    guard.update(|e| {
                        let _ = e.intent.observe_shared_access(&id, &value);
                    });
                    observe_previous_share(&guard.key, &id);
                }
                Some(false) => {}
                None => return Err("This person’s access response was incomplete."),
            }
        } else {
            let label = users::selected_labels(&state.peek(), std::slice::from_ref(&id))
                .first()
                .map(|v| v.label.clone())
                .unwrap_or_else(|| id.clone());
            if let Some(mut person) = previous_share(&guard.key, &id) {
                person.label = label;
                if person.result == ShareResult::Sending {
                    person.result = ShareResult::Uncertain;
                }
                guard.update(|e| e.recipients.push(person));
                continue;
            }
            guard.update(|e| {
                e.recipients.push(Recipient {
                    id,
                    label,
                    grant: AssigneeGrant::Write,
                    result: ShareResult::NotAttempted,
                    dm_attempted: false,
                    channel: None,
                })
            });
        }
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    guard: &Guard,
    c: &Context,
    action: Action,
) -> WorkResult<Option<EditAttempt>> {
    fresh(state, guard, c).await?;
    access(state, guard, c).await?;
    if guard
        .entry()
        .unwrap()
        .intent
        .sharing_needed()
        .unwrap()
        .is_empty()
    {
        return admit(
            &mut state.write(),
            guard,
            TaskAssignmentDecision::ExistingAccess,
        )
        .map(Some);
    }
    if action == Action::Share {
        let response = read(
            state,
            guard,
            c,
            api::list_experiments(&c.client, &c.session),
        )
        .await?;
        let experiments = api::assignee_share_experiments(&response)
            .map_err(|_| "Sharing experiments could not be verified.")?;
        let people = share_candidates(&guard.entry().unwrap())?;
        for person in people {
            if person.result != ShareResult::NotAttempted {
                continue;
            }
            if person.dm_attempted && person.channel.is_none() {
                return Err(
                    "The earlier DM open was unconfirmed. It will not be repeated. You can choose Not Now.",
                );
            }
            let channel = if let Some(channel) = person.channel {
                channel
            } else if let Some(channel) = cached_dm(&state.peek(), &guard.scope.team, &person.id)? {
                guard.update(|e| {
                    e.recipients
                        .iter_mut()
                        .find(|p| p.id == person.id)
                        .unwrap()
                        .channel = Some(channel.clone())
                });
                channel
            } else {
                // Resolve only on explicit Share. This may open a DM; retain the
                // outcome even when this editor retires before the reply.
                if !guard.current(&state.peek()) {
                    return Err("The editor was replaced before sharing.");
                }
                guard.update(|e| {
                    e.recipients
                        .iter_mut()
                        .find(|p| p.id == person.id)
                        .unwrap()
                        .dm_attempted = true
                });
                let reply = read(
                    state,
                    guard,
                    c,
                    api::list_assignee_open_dm(&c.client, &c.session, &person.id),
                )
                .await?;
                let channel=api::assignee_dm(&reply,&person.id).map_err(|_|"The recipient DM could not be verified. Its open request will not be repeated.")?;
                guard.update(|e| {
                    e.recipients
                        .iter_mut()
                        .find(|p| p.id == person.id)
                        .unwrap()
                        .channel = Some(channel.clone())
                });
                channel
            };
            let id = channel["id"]
                .as_str()
                .ok_or("The recipient DM is missing.")?;
            let response = read(
                state,
                guard,
                c,
                api::list_assignee_channel(&c.client, &c.session, id),
            )
            .await?;
            let channel =
                api::assignee_dm(&response, &person.id).map_err(|_| "The recipient DM changed.")?;
            if channel["id"].as_str() != Some(id) {
                return Err("The recipient DM identity changed.");
            }
            let prefs = if enterprise(c) && channel.get("is_ext_shared") == Some(&Value::Bool(true))
            {
                Some(
                    read(
                        state,
                        guard,
                        c,
                        api::list_assignee_prefs(&c.client, &c.session, id),
                    )
                    .await?,
                )
            } else {
                None
            };
            let policy =
                api::assignee_share_policy(&channel, experiments, enterprise(c), prefs.as_ref())
                    .map_err(|_| "External sharing policy could not be verified.")?;
            let allowed = match policy {
                api::AssigneeSharePolicy::Allowed => true,
                api::AssigneeSharePolicy::Blocked => false,
                api::AssigneeSharePolicy::CheckUploads => {
                    let response = read(
                        state,
                        guard,
                        c,
                        api::list_assignee_can_share(&c.client, &c.session, &guard.key.list, id),
                    )
                    .await?;
                    api::assignee_upload_share_allowed(&response)
                        .map_err(|_| "External upload sharing policy could not be verified.")?
                }
            };
            if !allowed {
                guard.update(|e| {
                    e.recipients
                        .iter_mut()
                        .find(|p| p.id == person.id)
                        .unwrap()
                        .result = ShareResult::Blocked
                });
                continue;
            }
            let team = api::assignee_context_team(&channel, experiments)
                .map_err(|_| "The recipient workspace context could not be verified.")?;
            fresh(state, guard, c).await?;
            guard.update(|e| {
                e.recipients
                    .iter_mut()
                    .find(|p| p.id == person.id)
                    .unwrap()
                    .result = ShareResult::Sending;
                e.message = "Sharing this List…".into();
            });
            let _ = state.write();
            let result = io::execute(
                state,
                guard,
                c,
                api::list_assignee_share(
                    &c.client,
                    &c.session,
                    &guard.key.list,
                    id,
                    person.grant,
                    team.as_deref(),
                ),
            )
            .await;
            // Receipt-only completion is allowed after retirement; it never
            // mutates a document, record, selected view, or another editor.
            let acknowledged = result
                .as_ref()
                .is_ok_and(|v| api::assignee_positive(v).is_ok());
            guard.update(|e| {
                let p = e.recipients.iter_mut().find(|p| p.id == person.id).unwrap();
                p.result = if acknowledged {
                    ShareResult::Acknowledged
                } else if matches!(&result, Err(ListAssigneeFailure::NotRequested(_))) {
                    ShareResult::NotRequested
                } else {
                    ShareResult::Uncertain
                };
                if let Err(
                    ListAssigneeFailure::NotRequested(error)
                    | ListAssigneeFailure::Unconfirmed(error),
                ) = &result
                {
                    cool_down(e, error);
                }
            });
            if let Err(
                ListAssigneeFailure::NotRequested(error) | ListAssigneeFailure::Unconfirmed(error),
            ) = &result
            {
                record_cooldown(&guard.key, error);
            }
            if !acknowledged {
                return Err(
                    "Sharing is unconfirmed or was not sent. No share request will be repeated. Check access or choose Not Now.",
                );
            }
            if !guard.current(&state.peek()) {
                return Err(
                    "The share belongs to an earlier editor. Reopen this assignment to check access; it will not be repeated.",
                );
            }
        }
        // Acknowledgement is not access proof, and cannot rebase document data.
        fresh(state, guard, c).await?;
        access(state, guard, c).await?;
        let e = guard.entry().unwrap();
        let missing = e.intent.sharing_needed().unwrap();
        if missing.iter().all(|id| {
            e.recipients
                .iter()
                .any(|p| &p.id == id && p.result == ShareResult::Blocked)
        }) && e
            .recipients
            .iter()
            .any(|p| p.result == ShareResult::Acknowledged)
        {
            return admit(
                &mut state.write(),
                guard,
                TaskAssignmentDecision::AssignWithoutSharing,
            )
            .map(Some);
        }
    }
    let e = guard.entry().unwrap();
    if e.intent.sharing_needed().unwrap().is_empty() {
        return admit(
            &mut state.write(),
            guard,
            TaskAssignmentDecision::ExistingAccess,
        )
        .map(Some);
    }
    guard.update(|e|{
        e.stage=if e.recipients.iter().any(|p|matches!(p.result,ShareResult::Acknowledged|ShareResult::Uncertain|ShareResult::Sending)){Stage::Uncertain}else{Stage::Review};
        e.message=if e.stage==Stage::Uncertain {"Access is still unconfirmed. Share requests will not be repeated. Check access or choose Not Now."}else{"These people do not have access to this List. Share with eligible people, or choose Not Now to assign without sharing."}.into();
    });
    Ok(None)
}

fn cached_dm(shell: &ShellState, team: &str, user: &str) -> WorkResult<Option<Value>> {
    let workspace = shell
        .core
        .workspaces
        .get(team)
        .ok_or("The current workspace is unavailable.")?;
    let mut channels = workspace
        .channels
        .values()
        .filter(|c| c.is_im && c.user.as_ref().is_some_and(|id| id.as_str() == user));
    let first = channels.next();
    if channels.next().is_some() {
        return Err("The cached recipient DM is ambiguous.");
    }
    first
        .map(|c| serde_json::to_value(c).map_err(|_| "The recipient DM could not be read."))
        .transpose()
}

// The review's recipient rows are retained receipts, not a fresh authorization
// to grant. A positive canSee check removes a recipient from this action only;
// the immutable selected assignee IDs remain unchanged.
fn share_candidates(entry: &Entry) -> WorkResult<Vec<Recipient>> {
    let missing = entry
        .intent
        .sharing_needed()
        .map_err(|_| "Wait for the access checks.")?;
    Ok(entry
        .recipients
        .iter()
        .filter(|p| missing.contains(&p.id) && p.result == ShareResult::NotAttempted)
        .cloned()
        .collect())
}

#[cfg(test)]
mod tests;
