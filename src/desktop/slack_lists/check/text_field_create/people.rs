//! People-only continuation of the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::people as cells};
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"People failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Users(original) = e.original.as_ref() else {
        return Err("Nonselection editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::User
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong People editor identity".into());
    }
    Ok(
        json!({"directory":{"query":e.directory.query,"candidates":e.directory.candidates.iter().map(|c|json!([c.id,c.label])).collect::<Vec<_>>(),"loading":e.directory.loading,"error":e.directory.error},"generation":e.generation,"row":e.row_id,"column":e.column_id,"original":original,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error}),
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
    oracle::exact(&ledger, index, &proof.docs[1], pages, sync, "U00000001")?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing People Table")?;
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
        .ok_or("Missing People cell")?;
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
        return Err("People projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    if matches!(index, 3 | 6) {
        if !model.is_null() {
            return Err("Resolved People editor retained".into());
        }
    } else {
        if model.is_null()
            || model["original"]
                != if index >= 4 {
                    json!(["U00000001"])
                } else {
                    json!([])
                }
            || model["disabled"] != json!(index != 0)
            || model["saving"] != json!(matches!(index, 1 | 4))
            || model["stale"] != json!(matches!(index, 2 | 5))
        {
            return Err("People model phase/original changed".into());
        }
        let expected_directory = if index <= 2 {
            json!({"query":"Alice","candidates":[["U00000001","Alice Example"]],"loading":false,"error":null})
        } else {
            json!({"query":"","candidates":oracle::candidates(),"loading":false,"error":null})
        };
        if model["directory"] != expected_directory {
            return Err("People actual cache query changed during cell workflow".into());
        }
        if index == 0
            && model["error"] != "This value cannot be saved. Check the value and choices."
        {
            return Err("Missing People no-op proof".into());
        }
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[oracle::LOADED[index]]
        || find(state)? != proof.find
    {
        return Err("People raw data/Find changed".into());
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
    if proof.stages.len() != 9 + index {
        return Err("People checkpoint order changed".into());
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
    let snapshot = snapshot.ok_or("People controller did not settle")?;
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listPeopleCreate.geometry({},{})",
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
                return Err("Resolved DOM People editor remained".into());
            }
        } else if geometry["owner"][key].as_str()
            != value.as_u64().map(|n| n.to_string()).as_deref()
        {
            return Err("People DOM owner differs".into());
        }
    }
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("People changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("People changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":9+index,"stage":name,"geometry":geometry,"cellLedger":snapshot["ledger"],"editor":snapshot["editor"],"ledger":creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    seed: &Value,
) -> Result<u64, String> {
    checked(d, "window.__listPeopleCreate.open()").await?;
    let mut generation = None;
    for _ in 0..150 {
        let value = editor(&state.peek())?;
        if let Some(g) = value["generation"].as_u64() {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let g = generation.ok_or("People editor absent")?;
    wait_dom(
        d,
        &format!("window.__listPeopleCreate.ready({},{g})", json!(seed)),
    )
    .await?;
    Ok(g)
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listPeopleCreate.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    wait_dom(d, &format!("window.__listPeopleCreate.chosen(null,{g})")).await?;
    click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(d, &format!("window.__listPeopleCreate.noop({g})")).await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || model["original"] != json!([])
        || model["disabled"] != false
        || cells::snapshot(&state.peek())? != before
    {
        return Err("People no-op changed worker or draft".into());
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
    let pages = oracle::pages(&proof.population, "U00000001")?;
    checked(d, include_str!("people/people.js")).await?;
    let initial = checked(d, "window.__listPeopleCreate.initial()").await?;
    let browse = open(state, d, &json!([])).await?;
    let missing_noop = noop(state, d, browse).await?;
    candidate_stage(state, d, proof, &sync, &creation, "start", browse).await?;
    candidate_stage(state, d, proof, &sync, &creation, "end", browse).await?;
    let before_cancel = cells::snapshot(&state.peek())?;
    query(state, d, browse, "Whitney").await?;
    checked(
        d,
        &format!("window.__listPeopleCreate.choose('W00000012',{browse})"),
    )
    .await?;
    wait_dom(
        d,
        &format!("window.__listPeopleCreate.chosen('W00000012',{browse})"),
    )
    .await?;
    click(d, "slack-list-editor-cancel", browse).await?;
    let canceled_draft = wait_dom(d, "window.__listPeopleCreate.returned(true)").await?;
    if cells::snapshot(&state.peek())? != before_cancel || population(state)? != proof.population {
        return Err("People draft Cancel changed raw worker".into());
    }
    let first = open(state, d, &json!([])).await?;
    if first == browse {
        return Err("People canceled generation reused".into());
    }
    let reopened_noop = noop(state, d, first).await?;
    query(state, d, first, "Alice").await?;
    let input = checked(
        d,
        &format!("window.__listPeopleCreate.choose('U00000001',{first})"),
    )
    .await?;
    wait_dom(
        d,
        &format!("window.__listPeopleCreate.chosen('U00000001',{first})"),
    )
    .await?;
    stage(state, d, proof, &pages, &sync, 0, &creation).await?;
    click(d, "slack-list-editor-save", first).await?;
    stage(state, d, proof, &pages, &sync, 1, &creation).await?;
    let held = cells::snapshot(&state.peek())?;
    let disabled = checked(
        d,
        &format!("window.__listPeopleCreate.disabledNoReplay({first})"),
    )
    .await?;
    if cells::snapshot(&state.peek())? != held {
        return Err("Disabled People controls replayed".into());
    }
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 2, &creation).await?;
    click(d, "slack-list-editor-refresh", first).await?;
    wait_dom(d, "window.__listPeopleCreate.returned()").await?;
    stage(state, d, proof, &pages, &sync, 3, &creation).await?;
    let second = open(state, d, &json!(["U00000001"])).await?;
    if first == second {
        return Err("People draft generation reused".into());
    }
    checked(d, &format!("window.__listPeopleCreate.remove({second})")).await?;
    let clear = wait_dom(
        d,
        &format!("window.__listPeopleCreate.chosen(null,{second})"),
    )
    .await?;
    click(d, "slack-list-editor-save", second).await?;
    stage(state, d, proof, &pages, &sync, 4, &creation).await?;
    fixture::complete_pending(&mut state.write());
    stage(state, d, proof, &pages, &sync, 5, &creation).await?;
    click(d, "slack-list-editor-refresh", second).await?;
    let returned = wait_dom(d, "window.__listPeopleCreate.returned()").await?;
    let third = open(state, d, &json!([])).await?;
    if third == first || third == second {
        return Err("Final People draft generation reused".into());
    }
    let empty_noop = noop(state, d, third).await?;
    click(d, "slack-list-editor-cancel", third).await?;
    let canceled = wait_dom(d, "window.__listPeopleCreate.returned(true)").await?;
    stage(state, d, proof, &pages, &sync, 6, &creation).await?;
    let geometry = checked(d, "window.__listPeopleCreate.geometry('cleared',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 6, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"pages":pages,"missingNoop":missing_noop,"reopenedNoop":reopened_noop,"draftCancel":canceled_draft,"input":input,"disabledNoReplay":disabled,"clear":clear,"returned":returned,"emptyNoop":empty_noop,"canceled":canceled,"ledger":final_state["ledger"],"geometry":geometry,"actualForms":13,"cellLocalPreflights":2,"cellLocalReadCompletions":2,"httpExecuted":0}),
    )
}

async fn query(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
    value: &str,
) -> Result<(), String> {
    let before = cells::snapshot(&state.peek())?;
    checked(
        d,
        &format!("window.__listPeopleCreate.query({},{g})", json!(value)),
    )
    .await?;
    wait_dom(
        d,
        &format!("window.__listPeopleCreate.queryReady({},{g})", json!(value)),
    )
    .await?;
    let expected = if value == "Alice" {
        json!([["U00000001", "Alice Example"]])
    } else {
        json!([["W00000012", "Whitney External"]])
    };
    for _ in 0..150 {
        let m = editor(&state.peek())?;
        if m["directory"]
            == json!({"query":value,"candidates":expected,"loading":false,"error":null})
        {
            if cells::snapshot(&state.peek())? != before {
                return Err("Cached People query issued cell work".into());
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual cached People query did not settle".into())
}
async fn candidate_stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    sync: &Value,
    creation: &Value,
    which: &str,
    g: u64,
) -> Result<(), String> {
    let (ordinal, name) = match which {
        "start" => (7, "people-candidates-start"),
        "end" => (8, "people-candidates-end"),
        _ => return Err("Unknown People candidate checkpoint".into()),
    };
    if proof.stages.len() != ordinal {
        return Err("People candidate order changed".into());
    }
    proof.stable(state, 1)?;
    let ledger = cells::snapshot(&state.peek())?;
    let pages = oracle::pages(&proof.population, "U00000001")?;
    oracle::exact(&ledger, 0, &proof.docs[1], &pages, sync, "U00000001")?;
    let model = editor(&state.peek())?;
    let expected = oracle::candidates();
    if model["generation"] != g
        || model["original"] != json!([])
        || model["directory"]
            != json!({"query":"","candidates":expected,"loading":false,"error":null})
        || model["disabled"] != false
    {
        return Err("Actual cached candidate inventory differs".into());
    }
    let geometry = checked(
        d,
        &format!("window.__listPeopleCreate.candidate({},{g})", json!(which)),
    )
    .await?;
    if geometry["inventory"] != expected
        || geometry["owner"]["visit"].as_str()
            != sync["visit"].as_u64().map(|n| n.to_string()).as_deref()
        || geometry["owner"]["selection"].as_str()
            != sync["selection"].as_u64().map(|n| n.to_string()).as_deref()
    {
        return Err("Candidate DOM/model owner differs".into());
    }
    let capture = harness::capture(d, name).await?;
    checked(
        d,
        &format!(
            "window.__listPeopleCreate.candidateVerify({},{g})",
            json!(which)
        ),
    )
    .await?;
    proof.stable(state, 1)?;
    if cells::snapshot(&state.peek())? != ledger
        || editor(&state.peek())? != model
        || rename_fixture::snapshot(&state.peek())? != *creation
    {
        return Err("Candidate capture changed custody".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"cellLedger":ledger,"editor":model,"ledger":creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
