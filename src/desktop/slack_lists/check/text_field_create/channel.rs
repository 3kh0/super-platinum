//! Channel-only continuation of the unchanged seven creation checkpoints.
use super::*;
use crate::slack_lists::{fixture, fixture::text_field_create::channel as cells};
mod oracle;
pub(super) use oracle::STAGES;
pub(super) fn failed_snapshot(shell: &ShellState) -> Value {
    match cells::snapshot(shell) {
        Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 512 * 1024) => {
            json!({"snapshot":v,"error":null})
        }
        Ok(_) => json!({"snapshot":null,"error":"Channel failure ledger exceeds bound"}),
        Err(e) => json!({"snapshot":null,"error":e.chars().take(1024).collect::<String>()}),
    }
}
fn editor(shell: &ShellState) -> Result<Value, String> {
    let Some(e) = crate::slack_lists::editor::presentation(shell) else {
        return Ok(Value::Null);
    };
    let crate::slack_lists::CellEditValue::Channels(original) = e.original.as_ref() else {
        return Err("Nonselection editor".into());
    };
    if e.kind != crate::slack_lists::editor::EditorKind::Channel
        || e.row_id != cells::ROW
        || e.column_id != cells::COLUMN
    {
        return Err("Wrong Channel editor identity".into());
    }
    Ok(
        json!({"directory":{"query":e.directory.query,"candidates":e.directory.candidates.iter().map(|c|json!([c.id,c.label])).collect::<Vec<_>>(),"loading":e.directory.loading,"error":e.directory.error},"generation":e.generation,"row":e.row_id,"column":e.column_id,"original":original,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error}),
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
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Channel Table")?;
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
        .ok_or("Missing Channel cell")?;
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
        return Err("Channel projection/creation isolation drift".into());
    }
    let model = editor(&shell)?;
    let step = index % 4;
    let group = index / 4;
    if step == 3 {
        if !model.is_null() {
            return Err("Resolved Channel editor retained".into());
        }
    } else {
        let original = if group == 0 {
            json!([])
        } else {
            oracle::value(group - 1)
        };
        if model.is_null()
            || model["original"] != original
            || model["disabled"] != json!(step != 0)
            || model["saving"] != json!(step == 1)
            || model["stale"] != json!(step == 2)
        {
            return Err("Channel phase/original changed".into());
        }
        let (query, candidates) = match group {
            0 => ("#ALPHA", json!([["C00000001", "#alpha-public"]])),
            1 => ("beta", json!([["G00000002", "#beta-private"]])),
            _ => ("", oracle::candidates()),
        };
        if model["directory"]
            != json!({"query":query,"candidates":candidates,"loading":false,"error":null})
        {
            return Err("Actual Channel query changed".into());
        }
        if matches!(index, 0 | 4)
            && model["error"] != "This value cannot be saved. Check the value and choices."
        {
            return Err("Missing Channel no-op proof".into());
        }
    }
    drop(shell);
    if document(state)? != proof.docs[1]
        || population(state)? != pages[oracle::LOADED[index]]
        || find(state)? != proof.find
    {
        return Err("Channel raw data/Find changed".into());
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
    if proof.stages.len() != 9 + index {
        return Err("Channel checkpoint order changed".into());
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
    let snapshot = snapshot.ok_or("Channel controller did not settle")?;
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listChannelCreate.geometry({},{})",
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
                return Err("Resolved DOM Channel editor remained".into());
            }
        } else if geometry["owner"][key].as_str()
            != value.as_u64().map(|n| n.to_string()).as_deref()
        {
            return Err("Channel DOM owner differs".into());
        }
    }
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Channel changed during geometry".into());
    }
    let capture = harness::capture(d, name).await?;
    if stable(state, proof, pages, sync, index, creation)? != snapshot {
        return Err("Channel changed during capture".into());
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
    checked(d, "window.__listChannelCreate.open()").await?;
    let mut generation = None;
    for _ in 0..150 {
        let value = editor(&state.peek())?;
        if let Some(g) = value["generation"].as_u64() {
            generation = Some(g);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let g = generation.ok_or("Channel editor absent")?;
    wait_dom(
        d,
        &format!("window.__listChannelCreate.ready({},{g})", json!(seed)),
    )
    .await?;
    Ok(g)
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, g: u64) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listChannelCreate.click({},{g})", json!(id)),
    )
    .await
}
async fn noop(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    g: u64,
    original: &Value,
) -> Result<Value, String> {
    let before = cells::snapshot(&state.peek())?;
    let desired = original
        .as_array()
        .ok_or("No-op seed not array")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or("No-op seed not ID"))
        .collect::<Result<Vec<_>, _>>()?;
    let typed_reason = cells::no_op_reason(&state.peek(), &desired)?;
    wait_dom(
        d,
        &format!(
            "window.__listChannelCreate.chosen({},{g})",
            original
                .as_array()
                .and_then(|v| v.first())
                .unwrap_or(&Value::Null)
        ),
    )
    .await?;
    click(d, "slack-list-editor-save", g).await?;
    let dom = wait_dom(
        d,
        &format!("window.__listChannelCreate.noop({g},{original})"),
    )
    .await?;
    let model = editor(&state.peek())?;
    if model["generation"] != g
        || model["error"] != "This value cannot be saved. Check the value and choices."
        || model["original"] != *original
        || model["disabled"] != false
        || cells::snapshot(&state.peek())? != before
    {
        return Err("Channel no-op changed worker or draft".into());
    }
    Ok(
        json!({"typedReason":typed_reason,"before":before,"after":cells::snapshot(&state.peek())?,"editor":model,"dom":dom}),
    )
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
    checked(d, include_str!("channel/channel.js")).await?;
    let initial = checked(d, "window.__listChannelCreate.initial()").await?;
    let browse = open(state, d, &json!([])).await?;
    let missing_noop = noop(state, d, browse, &json!([])).await?;
    candidate_stage(state, d, proof, &sync, &creation, "start", browse).await?;
    candidate_stage(state, d, proof, &sync, &creation, "end", browse).await?;
    let before_cancel = cells::snapshot(&state.peek())?;
    query(state, d, browse, "#GAMMA").await?;
    checked(
        d,
        &format!("window.__listChannelCreate.choose('C00000003',{browse})"),
    )
    .await?;
    wait_dom(
        d,
        &format!("window.__listChannelCreate.chosen('C00000003',{browse})"),
    )
    .await?;
    click(d, "slack-list-editor-cancel", browse).await?;
    let canceled_draft = wait_dom(d, "window.__listChannelCreate.returned(true)").await?;
    if cells::snapshot(&state.peek())? != before_cancel || population(state)? != proof.population {
        return Err("Channel Cancel changed raw worker".into());
    }
    let mut generations = vec![browse];
    let mut cycles = vec![];
    for index in 0..3 {
        let seed = if index == 0 {
            json!([])
        } else {
            oracle::value(index - 1)
        };
        let g = open(state, d, &seed).await?;
        if generations.contains(&g) {
            return Err("Channel editor generation reused".into());
        }
        generations.push(g);
        let no_op = if index <= 1 {
            Some(noop(state, d, g, &seed).await?)
        } else {
            None
        };
        let input = if index < 2 {
            let (query_text, id) = if index == 0 {
                ("#ALPHA", "C00000001")
            } else {
                ("beta", "G00000002")
            };
            query(state, d, g, query_text).await?;
            let action = checked(
                d,
                &format!("window.__listChannelCreate.choose({},{g})", json!(id)),
            )
            .await?;
            wait_dom(
                d,
                &format!("window.__listChannelCreate.chosen({},{g})", json!(id)),
            )
            .await?;
            action
        } else {
            let action = checked(d, &format!("window.__listChannelCreate.remove({g})")).await?;
            wait_dom(d, &format!("window.__listChannelCreate.chosen(null,{g})")).await?;
            action
        };
        stage(state, d, proof, &pages, &sync, index * 4, &creation).await?;
        click(d, "slack-list-editor-save", g).await?;
        stage(state, d, proof, &pages, &sync, index * 4 + 1, &creation).await?;
        let held = cells::snapshot(&state.peek())?;
        let disabled = checked(
            d,
            &format!("window.__listChannelCreate.disabledNoReplay({g})"),
        )
        .await?;
        if cells::snapshot(&state.peek())? != held {
            return Err("Disabled Channel controls replayed".into());
        }
        fixture::complete_pending(&mut state.write());
        stage(state, d, proof, &pages, &sync, index * 4 + 2, &creation).await?;
        click(d, "slack-list-editor-refresh", g).await?;
        let returned = wait_dom(d, "window.__listChannelCreate.returned()").await?;
        let mut final_noop = Value::Null;
        if index == 2 {
            let empty = open(state, d, &json!([])).await?;
            if generations.contains(&empty) {
                return Err("Final Channel generation reused".into());
            }
            generations.push(empty);
            let no_op = noop(state, d, empty, &json!([])).await?;
            click(d, "slack-list-editor-cancel", empty).await?;
            let canceled = wait_dom(d, "window.__listChannelCreate.returned(true)").await?;
            final_noop = json!({"noop":no_op,"canceled":canceled});
        }
        stage(state, d, proof, &pages, &sync, index * 4 + 3, &creation).await?;
        cycles.push(json!({"generation":g,"noop":no_op,"input":input,"disabledNoReplay":disabled,"returned":returned,"finalNoop":final_noop}));
    }
    let geometry = checked(d, "window.__listChannelCreate.geometry('cleared',null)").await?;
    let final_state = stable(state, proof, &pages, &sync, 11, &creation)?;
    Ok(
        json!({"sync":sync,"initial":initial,"pages":pages,"missingNoop":missing_noop,"draftCancel":canceled_draft,"generations":generations,"cycles":cycles,"ledger":final_state["ledger"],"geometry":geometry,"actualForms":14,"cellLocalPreflights":3,"cellLocalReadCompletions":3,"httpExecuted":0}),
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
        &format!("window.__listChannelCreate.query({},{g})", json!(value)),
    )
    .await?;
    wait_dom(
        d,
        &format!(
            "window.__listChannelCreate.queryReady({},{g})",
            json!(value)
        ),
    )
    .await?;
    let expected = match value {
        "#ALPHA" => json!([["C00000001", "#alpha-public"]]),
        "beta" => json!([["G00000002", "#beta-private"]]),
        "#GAMMA" => json!([["C00000003", "#gamma-external"]]),
        _ => return Err("Unknown Channel query".into()),
    };

    for _ in 0..150 {
        let m = editor(&state.peek())?;
        if m["directory"]
            == json!({"query":value,"candidates":expected,"loading":false,"error":null})
        {
            if cells::snapshot(&state.peek())? != before {
                return Err("Cached Channel query issued cell work".into());
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual cached Channel query did not settle".into())
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
        "start" => (7, "channel-candidates-start"),
        "end" => (8, "channel-candidates-end"),
        _ => return Err("Unknown Channel candidate checkpoint".into()),
    };
    if proof.stages.len() != ordinal {
        return Err("Channel candidate order changed".into());
    }
    proof.stable(state, 1)?;
    let ledger = cells::snapshot(&state.peek())?;
    let pages = oracle::pages(&proof.population)?;
    oracle::exact(&ledger, 0, &proof.docs[1], &pages, sync)?;
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
        &format!("window.__listChannelCreate.candidate({},{g})", json!(which)),
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
            "window.__listChannelCreate.candidateVerify({},{g})",
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
