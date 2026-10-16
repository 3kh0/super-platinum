//! Seven Checkbox-only stages after the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::fixture::{self, text_field_create::checkbox as cells};
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    bounded_failure_snapshot(cells::snapshot(shell))
}
fn bounded_failure_snapshot(snapshot: Result<Value, String>) -> Value {
    match snapshot {
        Ok(value) => match serde_json::to_vec(&value) {
            Ok(bytes) if bytes.len() <= 512 * 1024 => json!({"snapshot":value,"error":null}),
            Ok(bytes) => {
                json!({"snapshot":null,"error":"Checkbox failure ledger exceeds 512 KiB","serializedBytes":bytes.len()})
            }
            Err(_) => {
                json!({"snapshot":null,"error":"Checkbox failure ledger serialization failed"})
            }
        },
        Err(error) => json!({"snapshot":null,"error":error.chars().take(1024).collect::<String>()}),
    }
}
fn projection_matches(cell: &crate::slack_lists::CellView, loaded: usize) -> bool {
    cell.checkbox == Some(loaded == 1) && cell.editable && cell.can_edit == (loaded != 0)
}

fn stable(
    state: Signal<ShellState>,
    proof: &Proof,
    pages: &[Value; 3],
    sync: &Value,
    index: usize,
    creation: &Value,
) -> Result<Value, String> {
    let shell = state.peek();
    let snapshot = cells::snapshot(&shell)?;
    oracle::exact(&snapshot, index, &proof.docs[1], pages, sync)?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Checkbox Table")?;
    let ids = p
        .entries
        .iter()
        .filter_map(|e| match e {
            crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let row = p
        .rows
        .iter()
        .find(|r| r.id == cells::ROW)
        .ok_or("Missing observed checkbox row")?;
    let cell = row
        .cells
        .iter()
        .find(|c| c.column_id == cells::COLUMN)
        .ok_or("Missing checkbox cell")?;
    let loaded = oracle::LOADED[index];
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.selected_view.as_deref() != Some("V_FILTER")
        || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
        || p.view_notice.is_some()
        || !p.incomplete
        || ids != super::oracle::order(1)
        || !projection_matches(cell, loaded)
        || p.loading
        || rename::presentation(&shell).is_some()
        || rename_fixture::snapshot(&shell)? != *creation
        || rename_fixture::text_create_registry(&shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Checkbox projection/creation isolation drift".into());
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[loaded]
        || find(state)? != proof.find
    {
        return Err("Checkbox raw loaded data/Find drift".into());
    }
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack_lists::{Document, RecordsPage};
    #[test]
    fn failed_cell_ledger_retains_actual_value_or_bounded_secondary_error() {
        let value = json!({"reads":[{"read":123,"after":{"read":0}}]});
        assert_eq!(
            bounded_failure_snapshot(Ok(value.clone())),
            json!({"snapshot":value,"error":null})
        );
        assert_eq!(
            bounded_failure_snapshot(Err("owner retired".into())),
            json!({"snapshot":null,"error":"owner retired"})
        );
        let large = bounded_failure_snapshot(Ok(json!("x".repeat(512 * 1024))));
        assert!(large["snapshot"].is_null());
        assert!(large["serializedBytes"].as_u64().unwrap() > 512 * 1024);
        assert_eq!(
            bounded_failure_snapshot(Err("x".repeat(2048)))["error"]
                .as_str()
                .unwrap()
                .len(),
            1024
        );
    }
    #[test]
    fn actual_policy_distinguishes_missing_input_from_existing_boolean_editability() {
        let doc=Document::parse(json!({"id":"F_LIST_FIXTURE","mode":"list","access":"owner","list_metadata":{"schema":[{"id":"C_CHECKBOX_OBSERVED","key":"checkbox_observed","name":"Verified","type":"checkbox"}],"views":[]}})).unwrap();
        let column = doc.column("C_CHECKBOX_OBSERVED").unwrap();
        for (index, fields) in [
            json!([]),
            json!([{"key":"checkbox_observed","checkbox":true}]),
            json!([{"key":"checkbox_observed","checkbox":false}]),
        ]
        .into_iter()
        .enumerate()
        {
            let page=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R_LIST_003","list_id":"F_LIST_FIXTURE","fields":fields}]})).unwrap();
            let row = page.record("R_LIST_003").unwrap();
            let input = column.checkbox_input_value_with_policy(row, &doc.id, None);
            let can_edit = doc.editable(Some("full")) && column.editable_value(row, &doc.id, None);
            let mut cell = crate::slack_lists::CellView {
                column_id: column.id.clone(),
                text: String::new(),
                checkbox: input,
                editable: input.is_some(),
                can_edit,
                content: crate::slack_lists::CellContent::Plain,
            };
            assert_eq!(can_edit, index != 0);
            assert!(projection_matches(&cell, index));
            cell.can_edit = !cell.can_edit;
            assert!(!projection_matches(&cell, index));
        }
    }
}
async fn settled(
    state: Signal<ShellState>,
    proof: &Proof,
    pages: &[Value; 3],
    sync: &Value,
    index: usize,
    creation: &Value,
) -> Result<Value, String> {
    for _ in 0..150 {
        // A recorded hook failure or retired scope is never a polling condition.
        cells::snapshot(&state.peek())?;
        if let Ok(v) = stable(state, proof, pages, sync, index, creation) {
            return Ok(v);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    stable(state, proof, pages, sync, index, creation)
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    pages: &[Value; 3],
    sync: &Value,
    index: usize,
    creation: &Value,
) -> Result<(), String> {
    if proof.stages.len() != 7 + index {
        return Err("Checkbox stage order changed".into());
    }
    let name = STAGES[index];
    let ledger = settled(state, proof, pages, sync, index, creation).await?;
    let checked = oracle::LOADED[index] == 1;
    let disabled = !matches!(index, 3 | 6);
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listCheckboxCreate.geometry({},{},{})",
            json!(name),
            checked,
            disabled
        ),
    )
    .await?;
    if geometry["owner"]["visit"].as_str()
        != sync["visit"].as_u64().map(|v| v.to_string()).as_deref()
        || geometry["owner"]["selection"].as_str()
            != sync["selection"].as_u64().map(|v| v.to_string()).as_deref()
    {
        return Err("Checkbox rendered owner differs".into());
    }
    if stable(state, proof, pages, sync, index, creation)? != ledger {
        return Err("Cell changed during geometry evaluation".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != ledger {
        return Err("Cell changed during native capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":7+index,"stage":name,"geometry":geometry,"cellLedger":ledger,"ledger":creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<Value, String> {
    proof.stable(state, 1)?;
    let creation = rename_fixture::snapshot(&state.peek())?;
    let sync = cells::arm(
        &mut state.write(),
        &proof.docs[0],
        &proof.docs[1],
        &proof.population,
        proof.id,
    )?;
    proof.stable(state, 1)?;
    let pages = oracle::pages(&proof.population)?;
    checked(d, include_str!("checkbox.js")).await?;
    proof.stable(state, 1)?;
    let initial = checked(d, "window.__listCheckboxCreate.initial()").await?;
    proof.stable(state, 1)?;
    let before = cells::snapshot(&state.peek())?;
    if before["preflights"] != json!([])
        || before["requests"] != json!([])
        || before["completed"] != json!([])
        || before["reads"] != json!([])
        || before["attemptCount"] != 0
        || !before["pending"].is_null()
        || !before["operation"].is_null()
        || before["server"] != pages[0]
    {
        return Err("Sync admitted a cell mutation".into());
    }
    let first = checked(d, "window.__listCheckboxCreate.toggle(true)").await?;
    stage(state, d, proof, &pages, &sync, 0, &creation).await?;
    fixture::set_outcome_uncertain(&mut state.write());
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 1, &creation).await?;
    cells::queue_missing(&mut state.write())?;
    let missing = checked(d, "window.__listCheckboxCreate.refresh()").await?;
    stage(state, d, proof, &pages, &sync, 2, &creation).await?;
    let missing_receipt = cells::snapshot(&state.peek())?;
    // Check settled paint twice while the unresolved receipt blocks another gesture.
    let no_replay = wait_dom(d, "window.__listCheckboxCreate.blocked()").await?;
    if stable(state, proof, &pages, &sync, 2, &creation)? != missing_receipt {
        return Err("Missing read replayed a cell write".into());
    }
    let true_read = checked(d, "window.__listCheckboxCreate.refresh()").await?;
    stage(state, d, proof, &pages, &sync, 3, &creation).await?;
    let second = checked(d, "window.__listCheckboxCreate.toggle(false)").await?;
    stage(state, d, proof, &pages, &sync, 4, &creation).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 5, &creation).await?;
    let false_read = checked(d, "window.__listCheckboxCreate.refresh()").await?;
    stage(state, d, proof, &pages, &sync, 6, &creation).await?;
    let terminal_geometry = checked(
        d,
        "window.__listCheckboxCreate.geometry('unchecked',false,false)",
    )
    .await?;
    let final_ledger = stable(state, proof, &pages, &sync, 6, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"before":before,"pages":pages,"actions":{"check":first,"missingRead":missing,"noReplay":no_replay,"trueRead":true_read,"uncheck":second,"falseRead":false_read},"ledger":final_ledger,"geometry":terminal_geometry,"actualForms":13,"cellLocalPreflights":2,"cellLocalReadCompletions":3,"httpExecuted":0}),
    )
}
