//! Number-only continuation of the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::number_precision as cells};
pub(super) mod creation;
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Number failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Number(original) = e.original.as_ref() else {
        return Err("Nonnumeric editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::Number
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong Number editor identity".into());
    }
    Ok(
        json!({"generation":e.generation,"row":e.row_id,"column":e.column_id,"original":original,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error}),
    )
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
    let ledger = cells::snapshot(&shell)?;
    oracle::exact(&ledger, index, &proof.docs[1], pages, sync)?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Number Table")?;
    let ids = p
        .entries
        .iter()
        .filter_map(|e| match e {
            crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let c = p
        .rows
        .iter()
        .find(|r| r.id == cells::ROW)
        .and_then(|r| r.cells.iter().find(|c| c.column_id == cells::COLUMN))
        .ok_or("Missing Number cell")?;
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.selected_view.as_deref() != Some("V_FILTER")
        || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
        || p.view_notice.is_some()
        || !p.incomplete
        || p.loading
        || ids != super::oracle::order(1)
        || c.checkbox.is_some()
        || !c.can_edit
        || c.text
            != if oracle::LOADED[index] == 1 {
                "-12.346"
            } else {
                ""
            }
        || rename::presentation(&shell).is_some()
        || rename_fixture::snapshot(&shell)? != *creation
        || rename_fixture::text_create_registry(&shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Number projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    if matches!(index, 3 | 8) {
        if !model.is_null() {
            return Err("Resolved Number editor retained".into());
        }
    } else {
        if model.is_null()
            || model["original"]
                != if index >= 4 {
                    json!(-12.3456)
                } else {
                    Value::Null
                }
            || model["disabled"] != json!(matches!(index, 1 | 2 | 6 | 7))
            || model["saving"] != json!(matches!(index, 1 | 6))
            || model["stale"] != json!(matches!(index, 2 | 7))
        {
            return Err("Number model phase/original changed".into());
        }
        if index == 0
            && model["error"] != "This value cannot be saved. Check the value and choices."
        {
            return Err("Missing Number no-op proof".into());
        }
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[oracle::LOADED[index]]
        || find(state)? != proof.find
    {
        return Err("Number raw data/Find changed".into());
    }
    Ok(json!({"ledger":ledger,"editor":model}))
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
        return Err("Number checkpoint order changed".into());
    }
    let mut snapshot = None;
    for _ in 0..150 {
        cells::snapshot(&state.peek())?;
        if let Ok(v) = stable(state, proof, pages, sync, index, creation) {
            snapshot = Some(v);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let snapshot = snapshot.ok_or("Number controller did not settle")?;
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listNumberCreate.geometry({},{})",
            json!(name),
            snapshot["editor"]["generation"]
        ),
    )
    .await?;
    for (key, value) in [
        ("visit", sync["visit"].clone()),
        ("selection", sync["selection"].clone()),
        ("generation", snapshot["editor"]["generation"].clone()),
    ] {
        if value.is_null() {
            if !geometry["owner"][key].is_null() {
                return Err("Resolved DOM Number editor remained".into());
            }
        } else if geometry["owner"][key].as_str()
            != value.as_u64().map(|n| n.to_string()).as_deref()
        {
            return Err("Number DOM owner differs".into());
        }
    }
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Number changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Number changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":7+index,"stage":name,"geometry":geometry,"cellLedger":snapshot["ledger"],"editor":snapshot["editor"],"ledger":creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    seed: &str,
) -> Result<u64, String> {
    checked(d, "window.__listNumberCreate.open()").await?;
    let mut generation = None;
    for _ in 0..150 {
        let value = editor(&state.peek())?;
        if let Some(g) = value["generation"].as_u64() {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let g = generation.ok_or("Number editor absent")?;
    wait_dom(
        d,
        &format!("window.__listNumberCreate.ready({},{g})", json!(seed)),
    )
    .await?;
    Ok(g)
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listNumberCreate.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    wait_dom(d, &format!("window.__listNumberCreate.empty({g})")).await?;
    click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(d, &format!("window.__listNumberCreate.noop({g})")).await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || model["original"] != Value::Null
        || model["disabled"] != false
        || cells::snapshot(&state.peek())? != before
    {
        return Err("Number no-op changed worker or draft".into());
    }
    Ok(json!({"before":before,"after":cells::snapshot(&state.peek())?,"editor":model,"dom":dom}))
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
    checked(d, include_str!("number_precision/number.js")).await?;
    let initial = checked(d, "window.__listNumberCreate.initial()").await?;
    let first = open(state, d, "").await?;
    let missing_noop = noop(state, d, first).await?;
    let input = checked(
        d,
        &format!("window.__listNumberCreate.input('-12.3456',{first})"),
    )
    .await?;
    stage(state, d, proof, &pages, &sync, 0, &creation).await?;
    click(d, "slack-list-editor-save", first).await?;
    stage(state, d, proof, &pages, &sync, 1, &creation).await?;
    let held = cells::snapshot(&state.peek())?;
    let disabled = checked(
        d,
        &format!("window.__listNumberCreate.disabledNoReplay({first})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != held {
        return Err("Disabled Number controls replayed".into());
    }
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 2, &creation).await?;
    click(d, "slack-list-editor-refresh", first).await?;
    wait_dom(d, "window.__listNumberCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 3, &creation).await?;
    let reopened = open(state, d, "-12.3456").await?;
    if reopened == first {
        return Err("Number reopen generation reused".into());
    }
    stage(state, d, proof, &pages, &sync, 4, &creation).await?;
    let before_cancel = cells::snapshot(&state.peek())?;
    click(d, "slack-list-editor-cancel", reopened).await?;
    let reopen_cancel = wait_dom(d, "window.__listNumberCreate.returned()").await?;
    if cells::snapshot(&state.peek())? != before_cancel {
        return Err("Unchanged Number Cancel wrote".into());
    }
    let second = open(state, d, "-12.3456").await?;
    if [first, reopened].contains(&second) {
        return Err("Clear editor generation reused".into());
    }
    let clear = checked(d, &format!("window.__listNumberCreate.input('',{second})")).await?;
    wait_dom(d, &format!("window.__listNumberCreate.empty({second})")).await?;
    stage(state, d, proof, &pages, &sync, 5, &creation).await?;
    click(d, "slack-list-editor-save", second).await?;
    stage(state, d, proof, &pages, &sync, 6, &creation).await?;
    let clear_held = cells::snapshot(&state.peek())?;
    let clear_disabled = checked(
        d,
        &format!("window.__listNumberCreate.disabledNoReplay({second})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != clear_held {
        return Err("Disabled Clear replayed".into());
    }
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 7, &creation).await?;
    click(d, "slack-list-editor-refresh", second).await?;
    let returned = wait_dom(d, "window.__listNumberCreate.returned()").await?;
    let third = open(state, d, "").await?;
    if [first, second, reopened].contains(&third) {
        return Err("Final Number draft generation reused".into());
    }
    let empty_noop = noop(state, d, third).await?;
    click(d, "slack-list-editor-cancel", third).await?;
    let canceled = wait_dom(d, "window.__listNumberCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 8, &creation).await?;
    let geometry = checked(d, "window.__listNumberCreate.geometry('cleared',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 8, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"pages":pages,"missingNoop":missing_noop,"input":input,"disabledNoReplay":disabled,"clear":clear,"clearDisabledNoReplay":clear_disabled,"reopened":{"generation":reopened,"cancel":reopen_cancel,"ledgerBefore":before_cancel},"returned":returned,"emptyNoop":empty_noop,"canceled":canceled,"ledger":final_state["ledger"],"geometry":geometry,"actualForms":13,"cellLocalPreflights":2,"cellLocalReadCompletions":2,"httpExecuted":0}),
    )
}
