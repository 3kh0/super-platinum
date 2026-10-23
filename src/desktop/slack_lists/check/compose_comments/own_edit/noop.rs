//! Actual Save with an unchanged rich buffer; no manufactured update receipt.
use super::*;
use data::restart as disk;
mod runs;

async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..500 {
        let value = snapshot(state, m);
        if predicate(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original no-op edit did not settle".into())
}
async fn inventory(state: Signal<ShellState>, blocks: &Value) -> Result<Value, String> {
    for _ in 0..250 {
        let value = disk::snapshot(state).await?;
        if value["blocks"] == *blocks && !value["seed"].is_null() && value["attempt"].is_null() {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original no-op buffer did not become durable without an attempt".into())
}
fn unchanged(model: &Value, proof: &Value) -> Result<(), String> {
    let received = &proof["source"]["received"];
    let current = model["comments"]["comments"]["pageMessages"]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["ts"] == data::RECEIVED_TS))
        .ok_or("Original comment missing")?;
    if model["fixture"]["received"] != *received
        || [
            "blocks",
            "text",
            "edited",
            "reactions",
            "user",
            "thread_ts",
            "edit_fixture_opaque",
        ]
        .iter()
        .any(|key| current[*key] != received[*key])
        || ["prepared", "updates", "responses", "futureRows", "releases"]
            .iter()
            .any(|key| model["fixture"][*key] != json!([]))
        || model["edit"]["attempts"] != json!([])
    {
        return Err("No-op Save changed original comment or created send work".into());
    }
    Ok(())
}
fn no_preflight(before: &Value, after: &Value) -> Result<(), String> {
    if before["comments"]["ledger"] != after["comments"]["ledger"]
        || before["fixture"]["policy"] != after["fixture"]["policy"]
    {
        return Err("No-op Save issued new preflight or policy reads".into());
    }
    Ok(())
}
async fn open_edit(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
) -> Result<(), String> {
    p["actions"]
        .as_array_mut()
        .unwrap()
        .push(command(d, "open", json!({"ts":data::RECEIVED_TS})).await?);
    editor(d, true).await?;
    let current = wait(state, m, |v| {
        v["capture"]["canSave"] == true && v["capture"]["busy"] == false
    })
    .await?;
    // The fixture compares actual native blocks with the original seed via the
    // full active snapshot as well; never install the seed into the editor.
    let mount = command(d, "remember-edit", Value::Null).await?;
    let blocks = mount["blocks"].clone();
    let expected: Vec<_> = p["source"]["received"]["blocks"]
        .as_array()
        .ok_or("Original rich source missing")?
        .iter()
        .filter(|b| b["type"] != "context")
        .cloned()
        .collect();
    if blocks != json!(expected) || current["edit"]["active"]["source"] != p["source"]["received"] {
        return Err("Native original rich seed or original source differs".into());
    }
    if let Some(expected) = p.get("originalBlocks") {
        if expected != &blocks {
            return Err("Fresh Edit did not restore the original rich seed".into());
        }
    } else {
        p["originalBlocks"] = blocks.clone();
    }
    let saved = inventory(state, &blocks).await?;
    p["opens"]
        .as_array_mut()
        .unwrap()
        .push(json!({"active":current["edit"]["active"],"mount":mount,"inventory":saved}));
    Ok(())
}
async fn action(
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    id: &str,
) -> Result<(), String> {
    let proof = button(d, id).await?;
    p["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":id,"native":proof}));
    Ok(())
}
async fn closed(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    editor(d, false).await?;
    wait(state, m, |v| v["edit"]["active"].is_null()).await?;
    let saved = disk::snapshot(state).await?;
    if !saved["seed"].is_null() || !saved["attempt"].is_null() {
        return Err("No-op return retained an edit seed or durable send attempt".into());
    }
    Ok(())
}
async fn take(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    stage: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    locked: bool,
) -> Result<(), String> {
    if !locked {
        stable_drafts(state, m).await?;
    }
    let geometry = wait_dom(d, &script("state", json!({"ts":data::RECEIVED_TS}))).await?;
    let row_native = checked(d, &format!(r#"(()=>{{const rows=[...document.querySelectorAll('[data-list-comment-ts]')].filter(n=>n.dataset.listCommentTs==={});const r=rows[0],edited=r?.querySelectorAll('.message-edited').length;return {{ok:rows.length===1&&edited===0,ts:r?.dataset.listCommentTs,text:r?.textContent,editedMarkers:edited}};}})()"#,json!(data::RECEIVED_TS))).await?;
    let before = snapshot(state, m);
    unchanged(&before, p)?;
    let (journal, saved) = if locked {
        (Value::Null, Value::Null)
    } else {
        (
            compose::journal_snapshot(state).await?,
            disk::snapshot(state).await?,
        )
    };
    if geometry["reply"]["sameNode"] != true
        || geometry["reply"]["blocks"] != p["replyBlocks"]
        || (!locked && journal["blocks"] != p["replyBlocks"])
        || (!locked && !saved["attempt"].is_null())
        || (!before["edit"]["active"].is_null() && geometry["edit"]["sameNode"] != true)
    {
        return Err("No-op edit changed independent native or durable custody".into());
    }
    let png = harness::capture(d, stage).await?;
    if before != snapshot(state, m)
        || (!locked
            && (journal != compose::journal_snapshot(state).await?
                || saved != disk::snapshot(state).await?))
    {
        return Err("No-op custody changed across capture".into());
    }
    rows.push(json!({"stage":stage,"geometry":geometry,"rowNative":row_native,"model":before,"journal":journal,"editInventory":saved,"proof":p,"capture":png}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if data::runs_named() {
        return runs::run(state, d).await;
    }
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    checked(d, include_str!("../../comments.js")).await?;
    checked(d, include_str!("../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let mut p = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],"actions":[],"opens":[],"storageControls":[]});
    let mut rows = Vec::new();
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    let case = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE")
        .map_err(|_| "Missing no-op case")?;
    open_edit(state, &m, d, &mut p).await?;
    if case == "own-edit-noop-storage" {
        take(state, &m, d, "editing", &p, &mut rows, false).await?;
        let hold = harness::storage_control(state, &readiness, 1, "hold").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(hold.clone());
        p["storage"] = json!({"hold":hold,"release":null});
        p["saveBefore"] = snapshot(state, &m);
        action(d, &mut p, "slack-list-comment-edit-save").await?;
        wait(state, &m, |v| {
            v["capture"]["finishing"] == true
                && v["capture"]["busy"] == false
                && v["capture"]["canCancel"] == true
        })
        .await?;
        p["saveAfter"] = snapshot(state, &m);
        no_preflight(&p["saveBefore"], &p["saveAfter"])?;
        wait_dom(d,"({ok:document.getElementById('slack-list-comment-edit-cancel')?.textContent==='Retry return'&&document.getElementById('slack-list-comment-edit-save')?.disabled===true})").await?;
        take(state, &m, d, "return-failed", &p, &mut rows, true).await?;
        let release = harness::storage_control(state, &readiness, 2, "release").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(release.clone());
        p["storage"]["release"] = release;
        action(d, &mut p, "slack-list-comment-edit-cancel").await?;
        closed(state, &m, d).await?;
        take(state, &m, d, "returned", &p, &mut rows, false).await?;
    } else {
        take(state, &m, d, "unchanged-edit", &p, &mut rows, false).await?;
        p["saveBefore"] = snapshot(state, &m);
        action(d, &mut p, "slack-list-comment-edit-save").await?;
        closed(state, &m, d).await?;
        p["saveAfter"] = snapshot(state, &m);
        no_preflight(&p["saveBefore"], &p["saveAfter"])?;
        take(state, &m, d, "unchanged-return", &p, &mut rows, false).await?;
        open_edit(state, &m, d, &mut p).await?;
        p["append"] = command(
            d,
            "append",
            json!({"text":" Temporary native change \u{65e5}\u{1f98a}"}),
        )
        .await?;
        if p["append"]["blocks"] == p["originalBlocks"] {
            return Err("Native edit did not change blocks".into());
        }
        inventory(state, &p["append"]["blocks"]).await?;
        take(state, &m, d, "modified", &p, &mut rows, false).await?;
        p["revert"] = checked(d, &format!("({})()", include_str!("noop.js"))).await?;
        if p["revert"]["before"]["blocks"] != p["append"]["blocks"]
            || p["revert"]["after"]["blocks"] != p["originalBlocks"]
        {
            return Err("Actual native Undo did not restore exact original rich values".into());
        }
        inventory(state, &p["originalBlocks"]).await?;
        wait(state, &m, |v| v["capture"]["canSave"] == true).await?;
        take(state, &m, d, "reverted", &p, &mut rows, false).await?;
        p["revertSaveBefore"] = snapshot(state, &m);
        action(d, &mut p, "slack-list-comment-edit-save").await?;
        closed(state, &m, d).await?;
        p["revertSaveAfter"] = snapshot(state, &m);
        no_preflight(&p["revertSaveBefore"], &p["revertSaveAfter"])?;
        take(state, &m, d, "reverted-return", &p, &mut rows, false).await?;
        let completed = snapshot(state, &m)["comments"]["ledger"]["completed"]
            .as_u64()
            .ok_or("Original completed read count missing")?;
        action(d, &mut p, "slack-list-comments-refresh").await?;
        wait(state, &m, |v| {
            v["comments"]["ledger"]["completed"] == completed + 4
        })
        .await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &m, d, "readback", &p, &mut rows, false).await?;
        open_edit(state, &m, d, &mut p).await?;
        take(state, &m, d, "reopened", &p, &mut rows, false).await?;
        action(d, &mut p, "slack-list-comment-edit-cancel").await?;
        closed(state, &m, d).await?;
        p["finalReturn"] = json!({"native":command(d,"state",json!({"ts":data::RECEIVED_TS})).await?,"inventory":disk::snapshot(state).await?,"journal":compose::journal_snapshot(state).await?});
    }
    let final_model = snapshot(state, &m);
    unchanged(&final_model, &p)?;
    let expected: &[&str] = if case == "own-edit-noop-storage" {
        &["draft", "editing", "return-failed", "returned"]
    } else {
        &[
            "draft",
            "unchanged-edit",
            "unchanged-return",
            "modified",
            "reverted",
            "reverted-return",
            "readback",
            "reopened",
        ]
    };
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("No-op stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":final_model,"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Exact editable rich JSON equality; not full Delta normalization parity.","Private same-process SQLite cleanup and closed requests, no live Slack."]}),
    )
}
