//! Original Save job owns its attempt and can mint one ACK child.
use super::*;
use super_platinum_core::{
    slack::{api, transport::ListColumnRenameFailure as Failure},
    slack_lists::{ViewRefinementSaveAttempt, ViewRefinementSavePermit},
};
mod preference;

pub(super) fn admit(
    shell: &ShellState,
    guard: &Guard,
    intent: &ViewRefinementSave,
    loaded: &super::super::network::Metadata,
) -> Option<(ViewRefinementSaveAttempt, ViewRefinementSavePermit)> {
    if !guard.current(shell, Stage::Preparing) || registry::blocked(&guard.key.owner) {
        return None;
    }
    if !with_registry(|r| {
        r.entries
            .get(&guard.key)
            .is_some_and(|e| matches!(e.intent.as_ref(),Some(Intent::Refinement(i)) if i==intent))
    }) {
        return None;
    }
    let mut attempt = ViewRefinementSaveAttempt::new(intent.clone());
    let permit = attempt
        .admit(
            &loaded.document,
            loaded.allow.as_deref(),
            Some(&loaded.personal),
        )
        .ok()?;
    let custody = Custody::capture(shell)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = Stage::Saving;
        e.message = "Saving view refinements…".into();
        e.refinement_receipt = Some(Receipt {
            token: next_id(),
            dismissed_on_ack: false,
            preference: Preference::NotRequested,
            intent: None,
            custody: Some(custody),
            acknowledged_id: None,
        });
    });
    Some((attempt, permit))
}
async fn dispatch(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: ViewRefinementSavePermit,
) -> Result<Value, Failure> {
    if !guard.current(&state.peek(), Stage::Saving) || registry::blocked(&guard.key.owner) {
        return Err(Failure::NotRequested(Error::Api(
            "refinement_save_retired".into(),
        )));
    }
    if super::super::super::fixture::enabled(&state.peek()) {
        return fixture::write(state, guard, Permit::Refinement(permit)).await;
    }
    let c = guard
        .scope
        .request(&state.peek())
        .ok_or(Failure::NotRequested(Error::TransportNotConfigured))?;
    c.transport
        .execute_list_view_refinement_save(
            api::list_view_refinement_save(&c.client, &c.session, permit),
            || guard.current(&state.peek(), Stage::Saving) && !registry::blocked(&guard.key.owner),
        )
        .await
}
/// New's view-change callback selects first, then starts the preference thunk.
/// This private proof authorizes only the original job's immediate completion;
/// selection never installs ACK metadata or clears the pending local inputs.
struct PendingSelection {
    token: u64,
    id: String,
    custody: Custody,
}
impl PendingSelection {
    fn current(&self, shell: &ShellState, guard: &Guard) -> bool {
        guard.account_current(shell)
            && self.custody.current(shell)
            && with_registry(|r| {
                r.entries.get(&guard.key).is_some_and(|e| {
                    e.serial == guard.serial
                        && e.job == Some(guard.job)
                        && e.stage == Stage::Saving
                        && e.refinement_receipt.as_ref().is_some_and(|receipt| {
                            receipt.token == self.token
                                && !receipt.dismissed_on_ack
                                && receipt.acknowledged_id.as_deref() == Some(&self.id)
                        })
                })
            })
    }
}
fn select_pending(shell: &mut ShellState, guard: &Guard, id: &str) -> Option<PendingSelection> {
    if !guard.current(shell, Stage::Saving) {
        return None;
    }
    let token = with_registry(|r| {
        r.entries
            .get(&guard.key)?
            .refinement_receipt
            .as_ref()
            .map(|r| r.token)
    })?;
    let view = shell.slack_lists.view.as_mut()?;
    view.layout.selected = Some(id.into());
    view.layout.pending_refinement = Some(id.into());
    view.layout.selection_generation = next_id();
    view.layout.selection = None;
    view.layout.manual = true;
    view.layout.generation = next_id();
    rebuild_rows(shell);
    let custody = Custody::capture(shell)?;
    with_registry(|r| {
        let receipt = r
            .entries
            .get_mut(&guard.key)
            .unwrap()
            .refinement_receipt
            .as_mut()
            .unwrap();
        receipt.acknowledged_id = Some(id.into());
        receipt.custody = Some(custody.clone());
    });
    Some(PendingSelection {
        token,
        id: id.into(),
        custody,
    })
}
/// Only the current originating Save can publish its own provider/reset/selection
/// transition. Retired results remain receipts and cannot start a child.
fn settle(
    shell: &mut ShellState,
    guard: &Guard,
    intent: &ViewRefinementSave,
    result: Result<Value, Failure>,
    ack_id: Option<&str>,
    pending: Option<&PendingSelection>,
) -> bool {
    let new = matches!(intent.destination(), ViewRefinementDestination::New { .. });
    let current = if new && ack_id.is_some() {
        pending.is_some_and(|p| Some(p.id.as_str()) == ack_id && p.current(shell, guard))
    } else {
        guard.current(shell, Stage::Saving)
    };
    let not_requested = matches!(&result, Err(Failure::NotRequested(_)));
    if let Err(Failure::NotRequested(e) | Failure::Unconfirmed(e)) = &result {
        registry::record(&guard.key.owner, e);
    }
    let retained = with_registry(|r| {
        let Some(e) = r.entries.get_mut(&guard.key).filter(|e| {
            e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Saving
        }) else {
            return false;
        };
        e.stage = if not_requested {
            Stage::NotRequested
        } else {
            Stage::Uncertain
        };
        e.raw_reply = result.ok();
        e.message=if not_requested {"The view Save was not sent. Close this review to start a new attempt."} else {"The view Save has a separate default-view preference result. Check current metadata; neither request will be repeated."}.into();
        true
    });
    if !retained || !current {
        return false;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    // Actual existing Save clears provider after its awaited thunk, including
    // caught failure. New clears only through the positive-ID completion.
    if !new || ack_id.is_some() {
        view.layout.board_order = None;
    }
    if new && ack_id.is_some() {
        view.layout.refinements.reset();
    }
    view.layout.generation = next_id();
    rebuild_rows(shell);
    let custody = Custody::capture(shell);
    with_registry(|r| {
        if let Some(receipt) = r
            .entries
            .get_mut(&guard.key)
            .and_then(|e| e.refinement_receipt.as_mut())
        {
            receipt.acknowledged_id = ack_id.map(str::to_owned);
            receipt.dismissed_on_ack = new && ack_id.is_some();
            receipt.custody = custody;
        }
    });
    true
}
pub(in crate::slack_lists::rename) async fn save(
    mut state: Signal<ShellState>,
    guard: Guard,
    intent: ViewRefinementSave,
) {
    let loaded = match super::super::network::metadata(state, &guard, Stage::Preparing).await {
        Ok(v) => v,
        Err(_) => {
            unsent(
                &mut state.write(),
                &guard,
                "Could not verify the view and permissions. Nothing was sent.",
            );
            return;
        }
    };
    // Source captures the full current preference map separately. An unavailable
    // preference read does not turn a metadata Save into a guessed map overwrite.
    let prefs = match preference::before(state, &guard).await {
        Ok(raw) => Some(raw),
        Err(Failure::NotRequested(error) | Failure::Unconfirmed(error)) => {
            registry::record(&guard.key.owner, &error);
            None
        }
    };
    let Some((mut attempt, permit)) = admit(&state.peek(), &guard, &intent, &loaded) else {
        unsent(
            &mut state.write(),
            &guard,
            "This view or its local refinements changed. Nothing was sent.",
        );
        return;
    };
    state.write();
    let result = dispatch(state, &guard, permit).await;
    let ack = match &result {
        Ok(reply) => attempt.acknowledge(reply),
        Err(error) => {
            attempt.finish(matches!(error, Failure::NotRequested(_)));
            None
        }
    };
    let ack_id = ack.as_ref().map(|a| a.view_id().to_owned());
    // Admission happens before our owned state transition, against the original
    // complete document and preference observation. The handoff is consumed once.
    let child = if guard.current(&state.peek(), Stage::Saving) {
        prefs
            .as_ref()
            .zip(ack)
            .and_then(|(prefs, ack)| preference::admit(prefs, ack, intent.original()))
    } else {
        None
    };
    // Actual ef selects before dispatch; T resets/provider-clear after ef returns.
    // Child admission retained the original canonical snapshot above. Publish only
    // pending selection here, transferring custody through this exact owned step.
    let pending = if matches!(intent.destination(), ViewRefinementDestination::New { .. }) {
        ack_id
            .as_deref()
            .and_then(|id| select_pending(&mut state.write(), &guard, id))
    } else {
        None
    };
    let mut parent_state = state;
    let parent_completion = async {
        let current = settle(
            &mut parent_state.write(),
            &guard,
            &intent,
            result,
            ack_id.as_deref(),
            pending.as_ref(),
        );
        fixture::finished(&guard, "write", false);
        // NewViewModal closes on positive-ID completion. The selected pending
        // view resolves through an independent current metadata read.
        if current
            && matches!(intent.destination(), ViewRefinementDestination::New { .. })
            && ack_id.is_some()
        {
            let proof = {
                let shell = parent_state.peek();
                shell.slack_lists.view.as_ref().map(|v|json!({"serial":guard.serial.to_string(),"visit":v.generation.to_string(),"selection":v.layout.selection_generation.to_string(),"id":v.layout.selected}))
            };
            if let Some(proof) = proof {
                dioxus::document::eval(&format!("window.__listRefinementReturn?.commit({proof})"));
            }
            super::super::check(parent_state, guard.serial);
        }
    };
    if let Some(child) = child {
        // The actual source starts eah+ before clearing provider/reset/closing.
        // Shared execute_list_rename_once has no await before its final fence
        // and HTTP dispatch. Poll that child first, then publish the parent's
        // completion without awaiting the preference response.
        tokio::join!(biased; preference::run(state, guard.clone(), child), parent_completion);
    } else {
        parent_completion.await;
    }
}

#[cfg(test)]
mod tests;
