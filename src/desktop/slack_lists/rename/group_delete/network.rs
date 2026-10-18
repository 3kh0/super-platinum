//! The retained source schedules schema at 300 ms and starts cells immediately.
//! Both requests use the shared one-use transport; neither awaits the other.
use super::*;
use super_platinum_core::slack::transport::ListColumnRenameFailure as Failure;
use super_platinum_core::slack_lists::BoardGroupDeleteAttempt;
type Result = std::result::Result<Value, Failure>;

pub(super) fn admit(
    shell: &ShellState,
    guard: &Guard,
    intent: &BoardGroupDelete,
    loaded: &super::super::network::Metadata,
    page: &RecordsPage,
) -> Option<(Permit, Option<Permit>)> {
    if !guard.current(shell, Stage::Preparing) || registry::blocked(&guard.key.owner) {
        return None;
    }
    let valid = with_registry(|r| {
        r.entries.get(&guard.key).is_some_and(
            |e| matches!(e.intent.as_ref(),Some(Intent::GroupDelete(i)) if i == intent),
        )
    });
    if !valid {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let mut fresh = view.loaded.as_ref()?.clone();
    fresh.document = loaded.document.clone();
    fresh.records = page.clone();
    fresh.allow_lists = loaded.allow.clone();
    fresh.personal_task_lists = Some(loaded.personal.clone());
    let context = card_order::context(shell, &fresh, intent.view_id())?;
    let (schema, cells) = BoardGroupDeleteAttempt::new(intent.clone())
        .admit(
            &fresh.document,
            &fresh.records,
            &context,
            fresh.allow_lists.as_deref(),
            fresh.personal_task_lists.as_ref(),
        )
        .ok()?;
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = Stage::Saving;
        e.group_delete_outcomes = Some(Outcomes {
            schema: BoardGroupDeletePhase::InFlight,
            cells: if cells.is_some() {
                BoardGroupDeletePhase::InFlight
            } else {
                BoardGroupDeletePhase::NotRequired
            },
        });
        e.message = "Deleting group and clearing matching items...".into();
    });
    Some((
        Permit::GroupDeleteSchema(schema),
        cells.map(Permit::GroupDeleteCells),
    ))
}

async fn dispatch(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
    schema: bool,
) -> Result {
    if !guard.current(&state.peek(), Stage::Saving) || registry::blocked(&guard.key.owner) {
        return Err(Failure::NotRequested(Error::Api(
            "group_delete_retired".into(),
        )));
    }
    if super::super::super::fixture::enabled(&state.peek()) {
        return fixture::write(state, guard, permit).await;
    }
    let Some(context) = guard.scope.request(&state.peek()) else {
        return Err(Failure::NotRequested(Error::TransportNotConfigured));
    };
    let request = permit.request(&context.client, &context.session);
    let current =
        || guard.current(&state.peek(), Stage::Saving) && !registry::blocked(&guard.key.owner);
    if schema {
        context
            .transport
            .execute_list_board_choice(request, current)
            .await
    } else {
        context
            .transport
            .execute_list_board_group_delete_cells(request, current)
            .await
    }
}

pub(super) fn completed(shell: &mut ShellState, guard: &Guard, schema: bool, result: Result) {
    if let Err(Failure::NotRequested(e) | Failure::Unconfirmed(e)) = &result {
        registry::record(&guard.key.owner, e);
    }
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&guard.key).filter(|e| {
            e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Saving
        }) else {
            return;
        };
        let Some(outcomes) = e.group_delete_outcomes.as_mut() else {
            return;
        };
        let phase = if schema {
            &mut outcomes.schema
        } else {
            &mut outcomes.cells
        };
        if *phase != BoardGroupDeletePhase::InFlight {
            return;
        }
        *phase = if matches!(result, Err(Failure::NotRequested(_))) {
            BoardGroupDeletePhase::NotRequested
        } else {
            BoardGroupDeletePhase::Unconfirmed
        };
        if outcomes.schema != BoardGroupDeletePhase::InFlight
            && outcomes.cells != BoardGroupDeletePhase::InFlight
        {
            let unsent = outcomes.schema == BoardGroupDeletePhase::NotRequested
                && matches!(
                    outcomes.cells,
                    BoardGroupDeletePhase::NotRequested | BoardGroupDeletePhase::NotRequired
                );
            e.stage = if unsent {
                Stage::NotRequested
            } else {
                Stage::Uncertain
            };
            e.message = if unsent { "Neither change was sent. Close this review and start a new attempt." }
                else { "Group deletion has separate results below. Check current groups and items. Neither request will be sent again." }.into();
        }
    });
    let _ = shell;
}

pub(in crate::slack_lists::rename) async fn save(
    mut state: Signal<ShellState>,
    guard: Guard,
    intent: BoardGroupDelete,
) {
    let loaded = match super::super::network::metadata(state, &guard, Stage::Preparing).await {
        Ok(v) => v,
        Err(_) => {
            unsent(
                &mut state.write(),
                &guard,
                "Could not verify the group and permissions. Nothing was sent.",
            );
            return;
        }
    };
    let page = match super::super::network::population(state, &guard, Stage::Preparing).await {
        Ok(v) => v,
        Err(_) => {
            unsent(
                &mut state.write(),
                &guard,
                "Could not verify the current items. Nothing was sent.",
            );
            return;
        }
    };
    let permits = admit(&state.peek(), &guard, &intent, &loaded, &page);
    let Some((schema, cells)) = permits else {
        unsent(
            &mut state.write(),
            &guard,
            "The group or loaded items changed. Nothing was sent.",
        );
        return;
    };
    state.write();
    let mut schema_state = state;
    let mut cells_state = state;
    let schema_job = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let result = dispatch(schema_state, &guard, schema, true).await;
        completed(&mut schema_state.write(), &guard, true, result);
    };
    let cells_job = async {
        if let Some(cells) = cells {
            let result = dispatch(cells_state, &guard, cells, false).await;
            completed(&mut cells_state.write(), &guard, false, result);
        }
    };
    tokio::join!(schema_job, cells_job);
    fixture::finished(&guard, "write", false);
}
