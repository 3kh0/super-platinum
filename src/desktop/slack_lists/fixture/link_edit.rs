//! Link fixture restores the exact predecessor schema and canonical rows.
use super::*;

// Server receipt/readback shape observed on the private verification List.
// Leave the submitted snake-case cells alone; only the simulated server output
// is camel-cased and enriched, so both receipt and canonical read are exercised.
pub(super) fn canonical_cells(cells: &Value) -> Value {
    let mut cells = cells.clone();
    for cell in cells.as_array_mut().into_iter().flatten() {
        let Some(items) = cell.get_mut("link").and_then(Value::as_array_mut) else {
            continue;
        };
        for item in items {
            let Some(object) = item.as_object_mut() else {
                continue;
            };
            for (snake, camel) in [
                ("original_url", "originalUrl"),
                ("display_name", "displayName"),
                ("display_as_url", "displayAsUrl"),
            ] {
                if let Some(value) = object.remove(snake) {
                    object.insert(camel.into(), value);
                }
            }
            object.insert("attachment".into(), json!({}));
        }
    }
    cells
}

pub(crate) struct LinkEditSnapshot {
    document: Value,
    records: Value,
}
pub(crate) fn add_link_edits(shell: &mut ShellState) -> Option<LinkEditSnapshot> {
    if !enabled(shell) {
        return None;
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = LinkEditSnapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
    };
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()?
        .push(json!({"id":"C_EDIT_LINK","key":"edit_link","name":"Reference","type":"link"}));
    fixture.document["list_metadata"]["views"].as_array_mut()?.push(json!({"id":"V_EDIT_LINKS","name":"Link editing","type":"table","position":"503","columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_EDIT_LINK","key":"edit_link","visible":true}],"filters":[{"key":"estimate","column_id":"C_NUMBER","operator":"less_than_or_equal","values":[2]}],"sorts":[]}));
    let record = fixture.records["records"]
        .as_array_mut()?
        .iter_mut()
        .find(|record| record["id"] == "R_LIST_000")?;
    for field in record["fields"]
        .as_array_mut()?
        .iter_mut()
        .filter(|field| field["key"] == "estimate")
    {
        field["number"] = json!([0]);
    }
    record["fields"].as_array_mut()?.push(json!({"column_id":"C_EDIT_LINK","key":"edit_link","link":[{"original_url":"https://example.com/original","display_name":"Original reference","display_as_url":false,"attachment":{"title":"Server title"}}]}));
    refresh(shell);
    Some(snapshot)
}
pub(crate) fn link_edit_value(shell: &ShellState) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    shell.slack_lists.fixture.as_ref()?.records["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == "R_LIST_000")?["fields"]
        .as_array()?
        .iter()
        .find(|field| field["key"] == "edit_link")?
        .get("link")
        .cloned()
}
pub(crate) fn restore_link_edits(shell: &mut ShellState, snapshot: LinkEditSnapshot) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    refresh(shell);
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack_lists::LinkValue;
    #[test]
    fn link_server_receipt_preserves_identity_and_values_in_camel_array_shape() {
        let submitted = json!([{"row_id":"row","column_id":"link","link":[{
            "original_url":"https://example.com","display_name":"Explicit","display_as_url":true
        }]}]);
        let received = canonical_cells(&submitted);
        assert_eq!(
            received,
            json!([{"row_id":"row","column_id":"link","link":[{
                "originalUrl":"https://example.com","displayName":"Explicit","displayAsUrl":true,"attachment":{}
            }]}])
        );
        assert!(submitted[0]["link"][0].get("original_url").is_some());
        let clear = json!([{"row_id":"row","column_id":"link","link":[]}]);
        assert_eq!(canonical_cells(&clear), clear);
    }
    fn value() -> CellEditValue {
        CellEditValue::Link(Some(LinkValue {
            original_url: "https://example.com/new".into(),
            display_name: Some("New name".into()),
            display_as_url: false,
        }))
    }
    #[test]
    fn link_cancel_reopen_cannot_accept_prior_normalization_result() {
        let mut shell =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
        let baseline = shell.slack_lists.fixture.as_ref().unwrap().records.clone();
        let snapshot = add_link_edits(&mut shell).unwrap();
        let visit = generation(&shell).unwrap();
        editor::begin_shell(&mut shell, "R_LIST_000", "C_EDIT_LINK", visit).unwrap();
        let old = editor::presentation(&shell).unwrap().generation;
        shell.slack_lists.view.as_mut().unwrap().editor = None;
        editor::begin_shell(&mut shell, "R_LIST_000", "C_EDIT_LINK", visit).unwrap();
        let fresh = editor::presentation(&shell).unwrap().generation;
        assert_ne!(old, fresh);
        assert!(editor::begin_save(&mut shell, old, value()).is_err());
        assert_eq!(attempt_count(&shell), 0);
        let attempt = editor::begin_save(&mut shell, fresh, value()).unwrap();
        assert_eq!(
            attempt.edit.cells()[0]["link"],
            json!([{"original_url":"https://example.com/new","display_name":"New name","display_as_url":false}])
        );
        restore_link_edits(&mut shell, snapshot);
        assert_eq!(
            shell.slack_lists.fixture.as_ref().unwrap().records,
            baseline
        );
    }
    #[test]
    fn link_normalization_completion_cannot_cross_account_or_revision() {
        for rotate in [false, true] {
            let mut shell =
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
            add_link_edits(&mut shell).unwrap();
            let visit = generation(&shell).unwrap();
            editor::begin_shell(&mut shell, "R_LIST_000", "C_EDIT_LINK", visit).unwrap();
            let draft = editor::presentation(&shell).unwrap().generation;
            if rotate {
                shell.core.active_team = None;
            } else {
                shell.slack_lists.view.as_mut().unwrap().revision += 1;
            }
            assert!(editor::begin_save(&mut shell, draft, value()).is_err());
            assert_eq!(attempt_count(&shell), 0);
        }
    }
}
