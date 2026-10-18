//! Explicit disconnected group-name boundary, using the shared ledger and worker.
use super::*;
mod observer;
pub(crate) use observer::*;
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_CHOICE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-board-choice" | "slack-list-board-choice-narrow")
        )
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    assert!(check_enabled() && enabled(shell));
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
        ..Default::default()
    };
}
pub(super) fn read_form(request: &PreparedRequest) -> Result<(), Error> {
    let url = url::Url::parse(&request.url)
        .map_err(|_| Error::Api("fixture_board_records_url".into()))?;
    if url.path() == "/api/lists.records.list" {
        let expected = vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "F_LIST_FIXTURE".into()),
            ("include_subtasks".into(), "true".into()),
            ("archived".into(), "false".into()),
            ("include_suggested".into(), "false".into()),
            ("_x_reason".into(), "get-list-records".into()),
        ];
        if request.method == "POST"
            && matches!(&request.body,RequestBody::Form(fields) if fields==&expected)
        {
            return Ok(());
        }
        return Err(Error::Api("fixture_board_records_form".into()));
    }
    view_order::read_form(request)
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
        || !permit.is_board_choice()
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let intent = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::BoardChoice(i))
                if i.view_id() == "V_LANE" && i.column_key() == "status" =>
            {
                Some(i.clone())
            }
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    // Exact named native scenario. Add/color use focused domain/host tests until
    // their own independent native sequences are authored.
    if intent.action()
        != &(super_platinum_core::slack_lists::BoardChoiceAction::Rename {
            value: "OPT_PROGRESS".into(),
            label: "In motion".into(),
        })
    {
        return Err(NotRequested(Error::Api(
            "fixture_board_choice_target".into(),
        )));
    }
    let original = intent.original().raw.clone();
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.requests.len() != 3
            || l.responses.len() != 3
            || l.server.as_ref() != Some(&original)
        {
            return Err(NotRequested(Error::Api(
                "fixture_board_choice_order".into(),
            )));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("key".into(), "status".into()),
        ("options".into(), intent.options().to_string()),
        ("backfill_missing_cells_with_default".into(), "true".into()),
        ("_x_reason".into(), "update-column".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(NotRequested(Error::Api("fixture_board_choice_form".into())));
    }
    if record(request).map_err(NotRequested)? != "/api/lists.columns.update" {
        return Err(NotRequested(Error::Api("fixture_board_choice_path".into())));
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
    let mut l = ledger().lock().unwrap();
    let schema = l.server.as_mut().unwrap()["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap();
    schema
        .iter_mut()
        .find(|c| c["id"] == intent.column_id())
        .unwrap()["options"] = intent.options().clone();
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_alias_flag_required() {
        for alias in ["slack-list-board-choice", "slack-list-board-choice-narrow"] {
            assert!(gate(Some("1"), Some(alias)));
            assert!(!gate(None, Some(alias)));
            assert!(!gate(Some("true"), Some(alias)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-lane-order")));
    }
    #[test]
    fn actual_builder_query_parameters_do_not_hide_the_records_read() {
        let client = super_platinum_core::slack::SlackClient::default();
        let mut request = super_platinum_core::slack::api::lists_records_list(
            &client,
            &session(),
            "F_LIST_FIXTURE",
        );
        assert!(url::Url::parse(&request.url).unwrap().query().is_some());
        assert!(read_form(&request).is_ok());
        let RequestBody::Form(fields) = &mut request.body else {
            panic!()
        };
        fields.push(("cursor".into(), "guessed".into()));
        assert!(read_form(&request).is_err());
    }
}
