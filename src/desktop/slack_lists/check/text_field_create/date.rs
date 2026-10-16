//! Date-only continuation of the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::date as cells};
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Date failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Date(original) = e.original.as_ref() else {
        return Err("Noncalendar editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::Date
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong Date editor identity".into());
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
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Date Table")?;
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
        .ok_or("Missing Date cell")?;
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
        || rename::presentation(&shell).is_some()
        || rename_fixture::snapshot(&shell)? != *creation
        || rename_fixture::text_create_registry(&shell)?
            != json!({"present":false,"serial":null,"job":null,"intent":false})
    {
        return Err("Date projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    if matches!(index, 3 | 6) {
        if !model.is_null() {
            return Err("Resolved Date editor retained".into());
        }
    } else {
        if model.is_null()
            || model["original"]
                != if index >= 4 {
                    json!("2028-02-29")
                } else {
                    Value::Null
                }
            || model["disabled"] != json!(index != 0)
            || model["saving"] != json!(matches!(index, 1 | 4))
            || model["stale"] != json!(matches!(index, 2 | 5))
        {
            return Err("Date model phase/original changed".into());
        }
        if index == 0
            && model["error"] != "This value cannot be saved. Check the value and choices."
        {
            return Err("Missing Date no-op proof".into());
        }
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[oracle::LOADED[index]]
        || find(state)? != proof.find
    {
        return Err("Date raw data/Find changed".into());
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
        return Err("Date checkpoint order changed".into());
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
    let snapshot = snapshot.ok_or("Date controller did not settle")?;
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listDateCreate.geometry({},{})",
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
                return Err("Resolved DOM Date editor remained".into());
            }
        } else if geometry["owner"][key].as_str()
            != value.as_u64().map(|n| n.to_string()).as_deref()
        {
            return Err("Date DOM owner differs".into());
        }
    }
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Date changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Date changed during capture".into());
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
    checked(d, "window.__listDateCreate.open()").await?;
    let mut generation = None;
    for _ in 0..150 {
        let value = editor(&state.peek())?;
        if let Some(g) = value["generation"].as_u64() {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let g = generation.ok_or("Date editor absent")?;
    wait_dom(
        d,
        &format!("window.__listDateCreate.ready({},{g})", json!(seed)),
    )
    .await?;
    Ok(g)
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listDateCreate.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    click(d, "slack-list-date-clear", g).await?;
    wait_dom(d, &format!("window.__listDateCreate.empty({g})")).await?;
    click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(d, &format!("window.__listDateCreate.noop({g})")).await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || model["original"] != Value::Null
        || model["disabled"] != false
        || cells::snapshot(&state.peek())? != before
    {
        return Err("Date no-op changed worker or draft".into());
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
    checked(d, include_str!("date.js")).await?;
    let initial = checked(d, "window.__listDateCreate.initial()").await?;
    let first = open(state, d, "").await?;
    let missing_noop = noop(state, d, first).await?;
    stage(state, d, proof, &pages, &sync, 0, &creation).await?;
    let input = checked(
        d,
        &format!("window.__listDateCreate.input('2028-02-29',{first})"),
    )
    .await?;
    click(d, "slack-list-editor-save", first).await?;
    stage(state, d, proof, &pages, &sync, 1, &creation).await?;
    let held = cells::snapshot(&state.peek())?;
    let disabled = checked(
        d,
        &format!("window.__listDateCreate.disabledNoReplay({first})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != held {
        return Err("Disabled Date controls replayed".into());
    }
    fixture::set_outcome_uncertain(&mut state.write());
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 2, &creation).await?;
    click(d, "slack-list-editor-refresh", first).await?;
    wait_dom(d, "window.__listDateCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 3, &creation).await?;
    let second = open(state, d, "2028-02-29").await?;
    if first == second {
        return Err("Date draft generation reused".into());
    }
    click(d, "slack-list-date-clear", second).await?;
    let clear = wait_dom(d, &format!("window.__listDateCreate.empty({second})")).await?;
    click(d, "slack-list-editor-save", second).await?;
    stage(state, d, proof, &pages, &sync, 4, &creation).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 5, &creation).await?;
    click(d, "slack-list-editor-refresh", second).await?;
    let returned = wait_dom(d, "window.__listDateCreate.returned()").await?;
    let third = open(state, d, "").await?;
    if third == first || third == second {
        return Err("Final Date draft generation reused".into());
    }
    let empty_noop = noop(state, d, third).await?;
    click(d, "slack-list-editor-cancel", third).await?;
    let canceled = wait_dom(d, "window.__listDateCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 6, &creation).await?;
    let geometry = checked(d, "window.__listDateCreate.geometry('cleared',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 6, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"pages":pages,"missingNoop":missing_noop,"input":input,"disabledNoReplay":disabled,"clear":clear,"returned":returned,"emptyNoop":empty_noop,"canceled":canceled,"ledger":final_state["ledger"],"geometry":geometry,"actualForms":13,"cellLocalPreflights":2,"cellLocalReadCompletions":2,"httpExecuted":0}),
    )
}
