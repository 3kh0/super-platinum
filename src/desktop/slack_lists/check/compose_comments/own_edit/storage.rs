//! Actual exclusive SQLite failures: capture, clear, pre-send and terminal proof.
use super::*;
use data::restart as disk;

const STAGES: &[&str] = &[
    "draft",
    "editing",
    "capture-failed",
    "capture-recovered",
    "return-failed",
    "returned",
    "reopened-edit",
    "begin-failed",
    "begin-recovered",
    "saving",
    "receipt-failed",
    "receipt-recovered",
    "reopened",
];
const READS: &[u64] = &[4, 8, 8, 8, 8, 8, 12, 16, 16, 20, 20, 20, 24];
const POLICIES: &[usize] = &[0, 4, 4, 4, 4, 4, 8, 12, 12, 16, 16, 16, 16];
const CAPTURE_FAILED: &str =
    "The original edit capture stopped. Retry capture before saving or leaving.";
const RETURN_FAILED: &str =
    "The edit could not finish local recovery. Retry return; both drafts are retained.";
const BEGIN_FAILED: &str = "The edit was not sent because its recovery record could not be saved. Try Save again after local storage recovers; both drafts are retained.";
const RECEIPT_FAILED: &str = "The save result is retained here but could not be saved locally. Retry Cancel or Check result after storage recovers; this attempt will not be sent again.";

async fn wait(
    state: Signal<ShellState>,
    model: &Model,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..750 {
        let value = snapshot(state, model);
        if expected(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Own edit storage did not settle: {}",
        snapshot(state, model)
    ))
}
async fn notice(d: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-comment-edit-status')?.textContent==={}}})",
            json!(text)
        ),
    )
    .await?;
    Ok(())
}
async fn control(d: &dioxus::desktop::DesktopContext, name: &str) -> Result<(), String> {
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-comment-edit-{name}')?.disabled===false}})"
        ),
    )
    .await?;
    Ok(())
}
async fn ready(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    wait(state, model, |v| {
        v["capture"]["canSave"] == true
            && v["capture"]["canCancel"] == true
            && v["capture"]["failed"] == false
            && v["capture"]["busy"] == false
    })
    .await?;
    control(d, "save").await
}
async fn durable(state: Signal<ShellState>, blocks: &Value) -> Result<Value, String> {
    for _ in 0..250 {
        let value = disk::snapshot(state).await?;
        if value["blocks"] == *blocks && !value["seed"].is_null() {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Exact edit blocks did not become durable".into())
}
async fn storage(
    state: Signal<ShellState>,
    readiness: &Value,
    sequence: u64,
    proof: &mut Value,
) -> Result<(), String> {
    let action = if sequence % 2 == 1 { "hold" } else { "release" };
    let ack = harness::storage_control(state, readiness, sequence, action).await?;
    proof["storageControls"]
        .as_array_mut()
        .ok_or("Storage proof missing")?
        .push(ack.clone());
    if action == "hold" {
        proof["storage"] = json!({"hold":ack,"release":null});
    } else {
        proof["storage"]["release"] = ack;
    }
    Ok(())
}
fn text_leaves(value: &Value, out: &mut String) -> Result<(), String> {
    match value["type"].as_str() {
        Some("text" | "plain_text") => {
            out.push_str(value["text"].as_str().ok_or("Missing fixture text")?)
        }
        Some("rich_text" | "rich_text_section" | "context") => {
            for child in value["elements"]
                .as_array()
                .ok_or("Missing fixture elements")?
            {
                text_leaves(child, out)?;
            }
        }
        _ => return Err("Unexpected final fixture body shape".into()),
    }
    Ok(())
}
async fn final_body(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
) -> Result<Value, String> {
    let original = data::snapshot(&state.peek());
    let ack = &original["responses"][0]["raw"];
    let row = &ack["message"];
    let mut blocks = proof["finalEditBlocks"]
        .as_array()
        .ok_or("Final edit missing")?
        .clone();
    blocks.extend(
        original["received"]["blocks"]
            .as_array()
            .ok_or("Original blocks missing")?
            .iter()
            .filter(|block| block["type"] == "context")
            .cloned(),
    );
    if ack["ok"] != true
        || row["blocks"] != json!(blocks)
        || original["updates"].as_array().map(Vec::len) != Some(1)
    {
        return Err(
            "Original accepted update does not contain the exact final edit and context".into(),
        );
    }
    wait(state, model, |value| {
        let comments = &value["comments"]["comments"];
        let known = comments["pageMessages"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["ts"] == data::RECEIVED_TS));
        known.is_some_and(|known| {
            [
                "ts",
                "thread_ts",
                "user",
                "blocks",
                "text",
                "edited",
                "reactions",
            ]
            .iter()
            .all(|key| known[*key] == row[*key])
        }) && (name != "reopened"
            || comments["page"]["messages"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|known| known == row)))
    })
    .await?;
    let current = snapshot(state, model);
    if name == "receipt-recovered"
        && !current["comments"]["comments"]["page"]["messages"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r == &original["received"]))
    {
        return Err("Local recovery rewrote the original HTTP receipt".into());
    }
    let mut expected = String::new();
    for block in &blocks {
        text_leaves(block, &mut expected)?;
    }
    wait_dom(d,&format!(r#"(()=>{{
        const ts={},expected={};
        const rows=[...document.querySelectorAll('[data-list-comment-ts]')].filter(n=>n.dataset.listCommentTs===ts);
        const bodies=rows.length===1?[...rows[0].querySelectorAll('.message-body')]:[];
        const body=bodies[0], walker=body&&document.createTreeWalker(body,NodeFilter.SHOW_TEXT);
        let text='',node;
        while(walker&&(node=walker.nextNode())){{if(!node.parentElement?.closest('.message-edited'))text+=node.nodeValue;}}
        return {{ok:bodies.length===1&&text===expected,ts,text,expectedText:expected,bodyCount:bodies.length}};
    }})()"#,json!(data::RECEIVED_TS),json!(expected))).await
}
async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let index = stages.len();
    if STAGES.get(index) != Some(&name) {
        return Err("Storage stage order changed".into());
    }
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == READS[index]
            && v["fixture"]["policy"]
                .as_array()
                .is_some_and(|p| p.len() == POLICIES[index])
            && v["fixture"]["updates"]
                .as_array()
                .is_some_and(|p| p.len() == usize::from(index >= 9))
    })
    .await?;
    let rendered_body = if matches!(name, "receipt-recovered" | "reopened") {
        final_body(state, model, d, name, proof).await?
    } else {
        Value::Null
    };
    let locked = !proof["storage"].is_null() && proof["storage"]["release"].is_null();
    // A locked database is inspected by the runner's owning connection only.
    // Opening a Journal here would block and could mask the product failure.
    let (journal, inventory) = if locked {
        (Value::Null, Value::Null)
    } else {
        (
            compose::journal_snapshot(state).await?,
            disk::snapshot(state).await?,
        )
    };
    let geometry = wait_dom(d, &script("state", json!({"ts":data::RECEIVED_TS}))).await?;
    let before = snapshot(state, model);
    if geometry["reply"]["blocks"] != proof["replyBlocks"]
        || (name != "reopened" && geometry["reply"]["sameNode"] != true)
        || (!locked && journal["blocks"] != proof["replyBlocks"])
    {
        return Err(format!("Independent original reply changed at {name}"));
    }
    if !before["edit"]["active"].is_null() {
        if geometry["edit"]["sameNode"] != true
            || geometry["edit"]["blocks"] != proof["editBlocks"]
            || before["edit"]["active"]["blocks"] != proof["editBlocks"]
            || before["capture"]["scope"] != proof["editRemembered"]["scope"]
        {
            return Err("Original edit node/buffer changed during storage recovery".into());
        }
        if before["capture"]["finishing"] != true
            && before["capture"]["mount"] != proof["editRemembered"]["mount"]
        {
            return Err("Storage recovery borrowed another native holder".into());
        }
        if !locked && inventory["blocks"] != proof["editBlocks"] {
            return Err("Recovered disk differs from actual native edit".into());
        }
    }
    let png = harness::capture(d, name).await?;
    if before != snapshot(state, model) {
        return Err(format!("Storage model changed during {name}"));
    }
    if !locked
        && (journal != compose::journal_snapshot(state).await?
            || inventory != disk::snapshot(state).await?)
    {
        return Err("Original journal changed during capture".into());
    }
    stages.push(
        json!({"stage":name,"geometry":geometry,"model":before,"journal":journal,"renderedBody":rendered_body,
        "editInventory":inventory,"proof":proof,"capture":png}),
    );
    Ok(())
}
async fn edit(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    proof: &mut Value,
) -> Result<(), String> {
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    ready(state, model, d).await?;
    proof["editBlocks"] = command(d, "append", json!({"text":text})).await?["blocks"].clone();
    durable(state, &proof["editBlocks"]).await?;
    ready(state, model, d).await?;
    proof["editRemembered"] = command(d, "remember-edit", Value::Null).await?;
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
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
    stable_drafts(state, &model).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let mut proof = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],
        "storage":null,"storageControls":[],"editBlocks":null});
    let mut stages = Vec::new();
    take(state, &model, d, "draft", &proof, &mut stages).await?;
    edit(
        state,
        &model,
        d,
        " Original edit B \u{65e5}\u{1f98a}",
        &mut proof,
    )
    .await?;
    proof["originalEditBlocks"] = proof["editBlocks"].clone();
    proof["originalEditRemembered"] = proof["editRemembered"].clone();
    take(state, &model, d, "editing", &proof, &mut stages).await?;

    storage(state, &readiness, 1, &mut proof).await?;
    proof["editBlocks"] = command(
        d,
        "append",
        json!({"text":" Newer edit C \u{4e2d}\u{1f642}"}),
    )
    .await?["blocks"]
        .clone();
    proof["newerEditBlocks"] = proof["editBlocks"].clone();
    wait(state, &model, |v| {
        v["capture"]["canRetry"] == true
            && v["capture"]["failed"] == true
            && v["capture"]["busy"] == false
            && v["edit"]["active"]["blocks"] == proof["editBlocks"]
    })
    .await?;
    notice(d, CAPTURE_FAILED).await?;
    control(d, "retry").await?;
    proof["blockedCapture"] = command(d, "blocked-controls", Value::Null).await?;
    take(state, &model, d, "capture-failed", &proof, &mut stages).await?;
    storage(state, &readiness, 2, &mut proof).await?;
    button(d, "slack-list-comment-edit-retry").await?;
    ready(state, &model, d).await?;
    durable(state, &proof["editBlocks"]).await?;
    take(state, &model, d, "capture-recovered", &proof, &mut stages).await?;

    storage(state, &readiness, 3, &mut proof).await?;
    button(d, "slack-list-comment-edit-cancel").await?;
    wait(state, &model, |v| {
        v["capture"]["finishing"] == true
            && v["capture"]["canCancel"] == true
            && v["capture"]["canSave"] == false
            && v["capture"]["busy"] == false
    })
    .await?;
    notice(d, RETURN_FAILED).await?;
    control(d, "cancel").await?;
    proof["blockedReturnSave"] = command(d, "double-save", Value::Null).await?;
    take(state, &model, d, "return-failed", &proof, &mut stages).await?;
    storage(state, &readiness, 4, &mut proof).await?;
    button(d, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    wait(state, &model, |v| v["edit"]["active"].is_null()).await?;
    take(state, &model, d, "returned", &proof, &mut stages).await?;
    edit(
        state,
        &model,
        d,
        " Final edit D \u{65e5}\u{1f98a}",
        &mut proof,
    )
    .await?;
    proof["finalEditBlocks"] = proof["editBlocks"].clone();
    take(state, &model, d, "reopened-edit", &proof, &mut stages).await?;

    storage(state, &readiness, 5, &mut proof).await?;
    button(d, "slack-list-comment-edit-save").await?;
    wait(state, &model, |v| {
        v["edit"]["attempts"][0]["phase"] == "not-sent" && v["capture"]["busy"] == false
    })
    .await?;
    notice(d, BEGIN_FAILED).await?;
    ready(state, &model, d).await?;
    take(state, &model, d, "begin-failed", &proof, &mut stages).await?;
    storage(state, &readiness, 6, &mut proof).await?;
    ready(state, &model, d).await?;
    durable(state, &proof["editBlocks"]).await?;
    take(state, &model, d, "begin-recovered", &proof, &mut stages).await?;
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    button(d, "slack-list-comment-edit-save").await?;
    wait_fixture(state, |v| v["held"] == true).await?;
    proof["blockedSaving"] = command(d, "double-save", Value::Null).await?;
    take(state, &model, d, "saving", &proof, &mut stages).await?;
    storage(state, &readiness, 7, &mut proof).await?;
    proof["updateRelease"] = data::release(&state.peek())?;
    wait(state, &model, |v| {
        v["edit"]["attempts"][1]["phase"] == "confirmed"
            && v["capture"]["canCancel"] == true
            && v["capture"]["busy"] == false
    })
    .await?;
    notice(d, RECEIPT_FAILED).await?;
    proof["blockedReceiptSave"] = command(d, "double-save", Value::Null).await?;
    take(state, &model, d, "receipt-failed", &proof, &mut stages).await?;
    storage(state, &readiness, 8, &mut proof).await?;
    button(d, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    wait(state, &model, |v| v["edit"]["active"].is_null()).await?;
    take(state, &model, d, "receipt-recovered", &proof, &mut stages).await?;
    // Prove local ACK publication above before exposing the same accepted row
    // to the subsequent canonical read. No second update is issued.
    data::publish(&state.peek())?;
    let published = data::snapshot(&state.peek());
    if published["publishedRow"] != published["responses"][0]["raw"]["message"]
        || published["updates"].as_array().map(Vec::len) != Some(1)
    {
        return Err("Canonical readback did not retain the single original accepted update".into());
    }
    proof["serverPublication"] = json!({"originalReceipt":published["responses"][0]["raw"],
        "publishedRow":published["publishedRow"],"updates":1});
    button(d, "slack-list-comments-back").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    stable_drafts(state, &model).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original reply was not safely forgettable".into());
    }
    click(d, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    write(d).await?;
    proof["runtimeForgotten"] = json!(true);
    take(state, &model, d, "reopened", &proof, &mut stages).await?;
    Ok(
        json!({"ok":true,"case":"own-edit-storage","fixture":readiness["fixture"],"readiness":readiness,
        "stages":stages,"final":snapshot(state,&model),"initialRecords":fixture::initial_records(&model),
        "noLiveNetwork":true,"limits":["Actual exclusive SQLite locks in the isolated closed fixture only.",
            "Original accepted update receipt is retained separately from local persistence; no second update is sent.",
            "This mounted recovery case does not simulate a process or OS crash."]}),
    )
}
