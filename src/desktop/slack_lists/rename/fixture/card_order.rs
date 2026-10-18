//! Named offline card reorder; the existing shared worker and ledger execute it.
use super::*;
mod observer;
pub(crate) use observer::*;
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_CARD_DRAG_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-board-card-drag" | "slack-list-board-card-drag-narrow")
        )
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    assert!(check_enabled() && enabled(shell));
    let page = shell
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
        board_records: Some(page),
        ..Default::default()
    };
}
pub(crate) fn conflict(shell: &ShellState) -> Result<(), String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected card drag required".into());
    }
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Uncertain)
        .ok_or("No uncertain card move")?;
    let Some(Intent::CardOrder(i)) = &e.intent else {
        return Err("No card intent".into());
    };
    let mut l = ledger().lock().unwrap();
    if l.pending.is_some() || l.held_read.is_some() || l.card_records_once.is_some() {
        return Err("Pending fixture response".into());
    }
    l.card_records_once = Some(i.original_records().raw.clone());
    Ok(())
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
        || !permit.is_card_order()
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let intent = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::CardOrder(i)) => Some(i.clone()),
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    if intent.view_id() != "V_LANE"
        || intent.record_id() != "R_LIST_002"
        || intent.desired_position() != Some("0000000001.V")
        || intent.cells().is_some()
    {
        return Err(NotRequested(Error::Api("fixture_card_target".into())));
    }
    let original = intent.original_records().raw.clone();
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.requests.len() != 4
            || l.responses.len() != 4
            || l.server.as_ref() != Some(&intent.original().raw)
            || l.board_records.as_ref() != Some(&original)
        {
            return Err(NotRequested(Error::Api("fixture_card_order".into())));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "R_LIST_002".into()),
        ("view_id".into(), "V_LANE".into()),
        ("position".into(), "0000000001.V".into()),
        ("_x_reason".into(), "reorder-record".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(f) if f==&expected) {
        return Err(NotRequested(Error::Api("fixture_card_form".into())));
    }
    if record(request).map_err(NotRequested)? != "/api/lists.records.reorder" {
        return Err(NotRequested(Error::Api("fixture_card_path".into())));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.pending = Some(tx);
        l.responses
            .push(json!({"syntheticFailure":"response_lost"}));
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_closed".into())))?;
    let mut page = original;
    let row = page["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "R_LIST_002")
        .unwrap();
    row["view_positions"]["V_LANE"] = json!("0000000001.V");
    assert!(intent.observes_desired(
        intent.original(),
        &RecordsPage::parse(page.clone()).unwrap()
    ));
    ledger().lock().unwrap().board_records = Some(page);
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_flag_are_closed() {
        for a in [
            "slack-list-board-card-drag",
            "slack-list-board-card-drag-narrow",
        ] {
            assert!(gate(Some("1"), Some(a)));
            assert!(!gate(Some("true"), Some(a)));
            assert!(!gate(None, Some(a)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-lane-order")));
    }
}
