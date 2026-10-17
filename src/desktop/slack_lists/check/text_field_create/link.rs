//! Link-only continuation of the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::link as cells};
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Link failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Link(original) = e.original.as_ref() else {
        return Err("Non-Link editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::Link
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong Link editor identity".into());
    }
    Ok(
        json!({"generation":e.generation,"row":e.row_id,"column":e.column_id,"original":cells::typed(original),"nameSeed":e.link_name_seed,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error}),
    )
}

fn stable(
    state: Signal<ShellState>,
    proof: &Proof,
    pages: &[Value; 4],
    sync: &Value,
    index: usize,
    creation: &Value,
) -> Result<Value, String> {
    let shell = state.peek();
    let ledger = cells::snapshot(&shell)?;
    oracle::exact(&ledger, index, &proof.docs[1], pages, sync)?;
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Link Table")?;
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
        .ok_or("Missing Link cell")?;
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
        return Err("Link projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    let step = index % 4;
    let group = index / 4;
    if step == 3 {
        if !model.is_null() {
            return Err("Resolved Link editor retained".into());
        }
    } else {
        let original = if group == 0 {
            Value::Null
        } else {
            oracle::value(group - 1)
        };
        let name = if group == 0 {
            ""
        } else {
            "R\u{e9}f\u{e9}rence \u{6771}\u{4eac}"
        };
        if model.is_null()
            || model["original"] != original
            || model["nameSeed"] != name
            || model["disabled"] != json!(step != 0)
            || model["saving"] != json!(step == 1)
            || model["stale"] != json!(step == 2)
        {
            return Err("Link model phase/original changed".into());
        }
        if index == 0
            && model["error"] != "This value cannot be saved. Check the value and choices."
        {
            return Err("Missing Link no-op proof".into());
        }
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[oracle::LOADED[index]]
        || find(state)? != proof.find
    {
        return Err("Link raw data/Find changed".into());
    }
    Ok(json!({"ledger":ledger,"editor":model}))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    pages: &[Value; 4],
    sync: &Value,
    index: usize,
    creation: &Value,
) -> Result<(), String> {
    if proof.stages.len() != 7 + index {
        return Err("Link checkpoint order changed".into());
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
    let snapshot = snapshot.ok_or("Link controller did not settle")?;
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listLinkCreate.geometry({},{})",
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
                return Err("Resolved DOM Link editor remained".into());
            }
        } else if geometry["owner"][key].as_str()
            != value.as_u64().map(|n| n.to_string()).as_deref()
        {
            return Err("Link DOM owner differs".into());
        }
    }
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Link changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Link changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":7+index,"stage":name,"geometry":geometry,"cellLedger":snapshot["ledger"],"editor":snapshot["editor"],"ledger":creation,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    seed: &Value,
) -> Result<u64, String> {
    checked(d, "window.__listLinkCreate.open()").await?;
    let mut generation = None;
    for _ in 0..150 {
        let value = editor(&state.peek())?;
        if let Some(g) = value["generation"].as_u64() {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let g = generation.ok_or("Link editor absent")?;
    wait_dom(
        d,
        &format!("window.__listLinkCreate.ready({},{g})", json!(seed)),
    )
    .await?;
    Ok(g)
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listLinkCreate.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    wait_dom(d, &format!("window.__listLinkCreate.empty({g})")).await?;
    click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(d, &format!("window.__listLinkCreate.noop({g})")).await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || !model["original"].is_null()
        || model["disabled"] != false
        || cells::snapshot(&state.peek())? != before
    {
        return Err("Link no-op changed worker or draft".into());
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
    checked(d, include_str!("link/link.js")).await?;
    let initial = checked(d, "window.__listLinkCreate.initial()").await?;
    let first = open(state, d, &Value::Null).await?;
    let missing_noop = noop(state, d, first).await?;
    let before = cells::snapshot(&state.peek())?;
    fill(d, first, "discard.example.com", "Discard this").await?;
    click(d, "slack-list-editor-cancel", first).await?;
    let canceled = wait_dom(d, "window.__listLinkCreate.returned(true)").await?;
    if cells::snapshot(&state.peek())? != before || population(state)? != proof.population {
        return Err("Link Cancel changed worker/raw".into());
    }
    let mut generations = vec![first];
    let mut gestures = vec![json!({"kind":"missingCancel","dom":canceled})];
    let mut inputs = vec![];
    let mut disabled = vec![];
    let mut returned = vec![];
    for attempt in 0..3 {
        let seed = if attempt == 0 {
            Value::Null
        } else {
            oracle::value(attempt - 1)
        };
        let mut g = open(state, d, &seed).await?;
        if generations.contains(&g) {
            return Err("Link generation reused".into());
        }
        generations.push(g);
        if attempt == 0 {
            let empty = noop(state, d, g).await?;
            gestures.push(json!({"kind":"reopenedNoop","receipt":empty}));
            inputs.push(
                fill(
                    d,
                    g,
                    "example.com/new",
                    "R\u{e9}f\u{e9}rence \u{6771}\u{4eac}",
                )
                .await?,
            );
        } else if attempt == 1 {
            let before = cells::snapshot(&state.peek())?;
            let dirty = fill(d, g, "http://example.com/new", "Discard renamed link").await?;
            let gate = wait_dom(d, &format!("window.__listLinkCreate.dirty({g})")).await?;
            click(d, "slack-list-editor-cancel", g).await?;
            let focus = wait_dom(d, "window.__listLinkCreate.returned(true)").await?;
            if cells::snapshot(&state.peek())? != before || population(state)? != pages[1] {
                return Err("Dirty Link Cancel changed worker/raw".into());
            }
            gestures
                .push(json!({"kind":"dirtyDisplayCancel","input":dirty,"gate":gate,"focus":focus}));
            g = open(state, d, &seed).await?;
            if generations.contains(&g) {
                return Err("Link canceled generation reused".into());
            }
            generations.push(g);
        }
        stage(state, d, proof, &pages, &sync, attempt * 4, &creation).await?;
        let button = [
            "slack-list-editor-save",
            "slack-list-link-display",
            "slack-list-link-remove",
        ][attempt];
        inputs.push(click(d, button, g).await?);
        stage(state, d, proof, &pages, &sync, attempt * 4 + 1, &creation).await?;
        let before = cells::snapshot(&state.peek())?;
        disabled.push(checked(d, &format!("window.__listLinkCreate.disabledNoReplay({g})")).await?);
        if cells::snapshot(&state.peek())? != before {
            return Err("Link disabled controls replayed".into());
        }
        fixture::complete_pending(&mut state.write());
        stage(state, d, proof, &pages, &sync, attempt * 4 + 2, &creation).await?;
        click(d, "slack-list-editor-refresh", g).await?;
        returned.push(wait_dom(d, "window.__listLinkCreate.returned()").await?);
        if attempt == 2 {
            let g = open(state, d, &Value::Null).await?;
            if generations.contains(&g) {
                return Err("Final Link generation reused".into());
            }
            generations.push(g);
            let empty = noop(state, d, g).await?;
            click(d, "slack-list-editor-cancel", g).await?;
            let focus = wait_dom(d, "window.__listLinkCreate.returned(true)").await?;
            gestures.push(json!({"kind":"removedNoopCancel","receipt":empty,"focus":focus}));
        }
        stage(state, d, proof, &pages, &sync, attempt * 4 + 3, &creation).await?;
    }
    let geometry = checked(d, "window.__listLinkCreate.geometry('removed',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 11, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"pages":pages,"missingNoop":missing_noop,"gestures":gestures,"inputs":inputs,"disabledNoReplay":disabled,"returned":returned,"generations":generations,"ledger":final_state["ledger"],"geometry":geometry,"actualForms":14,"cellLocalPreflights":3,"cellLocalReadCompletions":3,"httpExecuted":0}),
    )
}
async fn fill(
    d: &dioxus::desktop::DesktopContext,
    g: u64,
    url: &str,
    name: &str,
) -> Result<Value, String> {
    let action = checked(
        d,
        &format!(
            "window.__listLinkCreate.fill({},{},{g})",
            json!(url),
            json!(name)
        ),
    )
    .await?;
    let settled = wait_dom(
        d,
        &format!(
            "window.__listLinkCreate.filled({},{},{g})",
            json!(url),
            json!(name)
        ),
    )
    .await?;
    Ok(json!({"action":action,"settled":settled}))
}
