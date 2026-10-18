//! Named offline card reorder; the existing shared worker and ledger execute it.
use super::*;
mod observer;
mod tail_observer;
pub(crate) use observer::*;
pub(crate) use tail_observer::{tail_callbacks, tail_drop_observed};
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_CARD_EXTENDED_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    Cross,
    Tail,
}
impl Case {
    pub(crate) fn row(self) -> &'static str {
        match self {
            Self::Cross => "R_LIST_002",
            Self::Tail => "R_LIST_070",
        }
    }
    pub(crate) fn destination(self) -> &'static str {
        match self {
            Self::Cross => "OPT_DONE",
            Self::Tail => "OPT_PROGRESS",
        }
    }
    pub(crate) fn before(self) -> Option<&'static str> {
        match self {
            Self::Cross => Some("R_LIST_005"),
            Self::Tail => None,
        }
    }
    pub(crate) fn position(self) -> &'static str {
        match self {
            Self::Cross => "0000000005.V",
            Self::Tail => "0000000074",
        }
    }
    pub(crate) fn query(self) -> &'static str {
        match self {
            Self::Cross => "Launch item 0",
            Self::Tail => "Launch item",
        }
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Cross => "Cross-lane card move",
            Self::Tail => "Loaded-page tail move",
        }
    }
    pub(crate) fn cells(self) -> Option<Value> {
        match self {
            Self::Cross => {
                Some(json!([{"row_id":"R_LIST_002","column_id":"C_SELECT","select":["OPT_DONE"]}]))
            }
            Self::Tail => None,
        }
    }
}
pub(crate) fn case_for(alias: &str) -> Option<Case> {
    match alias {
        "slack-list-board-card-cross" | "slack-list-board-card-cross-narrow" => Some(Case::Cross),
        "slack-list-board-card-tail" | "slack-list-board-card-tail-narrow" => Some(Case::Tail),
        _ => None,
    }
}
pub(crate) fn case() -> Option<Case> {
    case_for(&std::env::var("SUPER_PLATINUM_FIXTURE").ok()?)
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1") && alias.and_then(case_for).is_some()
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    assert!(check_enabled() && enabled(shell));
    tail_observer::reset();
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
    let mode = case().ok_or(NotRequested(Error::TransportNotConfigured))?;
    if intent.view_id() != "V_LANE"
        || intent.record_id() != mode.row()
        || intent.desired_position() != Some(mode.position())
        || intent.cells() != mode.cells()
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
    let mut expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), mode.row().into()),
        ("view_id".into(), "V_LANE".into()),
        ("position".into(), mode.position().into()),
        ("_x_reason".into(), "reorder-record".into()),
    ];
    if let Some(cells) = mode.cells() {
        expected.insert(5, ("cells".into(), cells.to_string()));
    }
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
        .find(|r| r["id"] == mode.row())
        .unwrap();
    row["view_positions"]["V_LANE"] = json!(mode.position());
    if mode == Case::Cross {
        row["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["key"] == "status")
            .unwrap()["select"] = json!(["OPT_DONE"]);
    }
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
            "slack-list-board-card-cross",
            "slack-list-board-card-cross-narrow",
            "slack-list-board-card-tail",
            "slack-list-board-card-tail-narrow",
        ] {
            assert!(gate(Some("1"), Some(a)));
            assert!(!gate(Some("true"), Some(a)));
            assert!(!gate(None, Some(a)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-lane-order")));
    }
}
