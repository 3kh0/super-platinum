//! Exact disconnected refinements boundary over the existing worker and ledger.
use super::*;
use super_platinum_core::slack_lists::ViewRefinementDestination;
mod forms;
mod observer;
pub(crate) use observer::*;
mod preference;
pub(in crate::slack_lists::rename) use preference::{read_preference, write_preference};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    Existing,
    New,
    Lost,
}
impl Case {
    pub(crate) fn created(self) -> bool {
        self != Self::Existing
    }
    fn view_id(self) -> &'static str {
        if self.created() {
            "V_REFINED_CREATED"
        } else {
            "V_LANE"
        }
    }
}
pub(crate) fn case_for(alias: &str) -> Option<Case> {
    match alias {
        "slack-list-board-refinement-save" | "slack-list-board-refinement-save-narrow" => {
            Some(Case::Existing)
        }
        "slack-list-board-refinement-save-new" | "slack-list-board-refinement-save-new-narrow" => {
            Some(Case::New)
        }
        "slack-list-board-refinement-save-lost"
        | "slack-list-board-refinement-save-lost-narrow" => Some(Case::Lost),
        _ => None,
    }
}
pub(crate) fn case() -> Option<Case> {
    case_for(&std::env::var("SUPER_PLATINUM_FIXTURE").ok()?)
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_BOARD_REFINEMENT_CHECK").as_deref() == Ok("1")
        && case().is_some()
}
fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected refinement checker required".into())
    }
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    require(shell).unwrap();
    let records = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        board_records: Some(records),
        refinement_preferences: Some(forms::original_preferences()),
        ..Default::default()
    };
}
pub(crate) fn release_preference(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    ledger()
        .lock()
        .unwrap()
        .refinement_child_pending
        .take()
        .ok_or("No held preference response")?
        .send(())
        .map_err(|_| "Preference receiver retired".into())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let l = ledger().lock().unwrap();
    Ok(
        json!({"requests":l.requests,"responses":l.responses,"finished":l.finished,"server":l.server,
        "population":l.board_records,"preferences":l.refinement_preferences,"events":l.refinement_events,
        "writePending":l.pending.is_some(),"readPending":l.held_read.is_some(),"preferencePending":l.refinement_child_pending.is_some()}),
    )
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    if !check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let mode = case().ok_or(NotRequested(Error::TransportNotConfigured))?;
    let intent = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::Refinement(i)) => Some(i.clone()),
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    if intent.view_id() != "V_LANE"
        || match intent.destination() {
            ViewRefinementDestination::Existing => mode.created(),
            ViewRefinementDestination::New { name } => {
                !mode.created() || name != "Focused delivery"
            }
        }
    {
        return Err(NotRequested(Error::Api("fixture_refinement_target".into())));
    }
    let request = permit.request(&state.peek().core.client, &session());
    forms::metadata(&request, mode).map_err(NotRequested)?;
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.refinement_child_pending.is_some()
            || l.requests.len() != 4
            || l.responses.len() != 4
            || l.server.as_ref() != Some(&intent.original().raw)
        {
            return Err(NotRequested(Error::Api("fixture_refinement_order".into())));
        }
    }
    record(request).map_err(NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let reply = {
        let mut l = ledger().lock().unwrap();
        let server = forms::saved(l.server.as_ref().unwrap(), mode).map_err(NotRequested)?;
        let reply = if mode == Case::Lost {
            json!({"syntheticFailure":"response_lost"})
        } else {
            json!({"ok":true,"list":server,"opaque_ack":{"keep":true}})
        };
        l.pending = Some(tx);
        l.responses.push(reply.clone());
        l.refinement_events
            .push(json!({"kind":"metadata-dispatched","serial":guard.serial}));
        reply
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_closed".into())))?;
    let mut l = ledger().lock().unwrap();
    l.server = Some(forms::saved(l.server.as_ref().unwrap(), mode).map_err(Unconfirmed)?);
    l.refinement_events
        .push(json!({"kind":"metadata-response-released","serial":guard.serial}));
    if mode == Case::Lost {
        Err(Unconfirmed(Error::Transport(
            "fixture_response_lost".into(),
        )))
    } else {
        Ok(reply)
    }
}
#[cfg(test)]
mod tests;
#[cfg(test)]
mod gate_tests {
    use super::*;
    #[test]
    fn six_dynamic_names_exclude_static_states_and_other_board_checkers() {
        for (name, want) in [
            ("slack-list-board-refinement-save", Case::Existing),
            ("slack-list-board-refinement-save-new", Case::New),
            ("slack-list-board-refinement-save-lost", Case::Lost),
        ] {
            assert_eq!(case_for(name), Some(want));
            assert_eq!(case_for(&format!("{name}-narrow")), Some(want));
        }
        for name in [
            "slack-list-board-refinement",
            "slack-list-board-refinement-new",
            "slack-list-board-lane-order",
            "",
        ] {
            assert_eq!(case_for(name), None);
        }
    }
}
