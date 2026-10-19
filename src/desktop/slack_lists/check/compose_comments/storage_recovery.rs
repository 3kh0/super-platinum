//! Real isolated SQLite contention and explicit local recovery through native controls.
use super::*;

const STAGES: &[&str] = &[
    "load-failed",
    "load-recovered",
    "save-failed",
    "back-refused",
    "save-recovered",
    "save-reopened",
    "receipt-failed",
    "receipt-recovered",
];

async fn observed(
    state: Signal<ShellState>,
    model: &Model,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    // Each product Journal open can wait five seconds; a flush can wait six.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let value = compose::snapshot(&state.peek(), model);
        if expected(&value) {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Storage recovery state did not settle: {value}"));
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}
fn draft(value: &Value) -> &Value {
    &value["draftStore"]["drafts"][0]
}
fn idle(value: &Value) -> bool {
    value["draftStore"]["drafts"]
        .as_array()
        .is_some_and(|rows| rows.len() == 1)
        && draft(value)["busy"] == false
        && draft(value)["saving"] == false
        && draft(value)["loading"] == false
}
async fn settled_error(
    state: Signal<ShellState>,
    model: &Model,
    terminal: bool,
) -> Result<Value, String> {
    observed(state, model, |v| {
        idle(v) && draft(v)["error"].is_string() && draft(v)["terminalPending"] == terminal
    })
    .await
}
async fn native_blocks(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let value=wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true;return v;})()").await?;
    Ok(value["editor"]["blocks"].clone())
}
async fn persisted(state: Signal<ShellState>, model: &Model, blocks: &Value) -> Result<(), String> {
    observed(state, model, |v| {
        idle(v)
            && draft(v)["blocks"] == *blocks
            && draft(v)["error"].is_null()
            && draft(v)["terminalPending"] == false
    })
    .await?;
    let journal = compose::journal_snapshot(state).await?;
    if journal["blocks"] != *blocks {
        return Err("Recovery did not persist exact native draft".into());
    }
    Ok(())
}
async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    mut proof: Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(rows.len()) != Some(&name) {
        return Err("Storage stage order changed".into());
    }
    observed(state, model, idle).await?;
    let failed = matches!(
        name,
        "load-failed" | "save-failed" | "back-refused" | "receipt-failed"
    );
    let expression = if name == "load-failed" {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&!v.editor&&v.notices.length>0&&!!document.getElementById('slack-list-comment-retry');return v;})()"
    } else if name == "receipt-recovered" {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&(!v.editor||v.editor.text.trim()==='');return v;})()"
    } else if failed {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true&&v.notices.length>0&&document.getElementById('slack-list-comment-send')?.disabled===true&&!!document.getElementById('slack-list-comment-retry');return v;})()"
    } else {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true&&document.getElementById('slack-list-comment-send')?.disabled===false;return v;})()"
    };
    let geometry = wait_dom(d, expression).await?;
    let locked = proof["storage"]["release"].is_null();
    proof["journal"] = if locked {
        Value::Null
    } else {
        compose::journal_snapshot(state).await?
    };
    let before = compose::snapshot(&state.peek(), model);
    if before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
    {
        return Err("Storage fixture acquired live state".into());
    }
    let capture = harness::capture(d, name).await?;
    if !locked && compose::journal_snapshot(state).await? != proof["journal"] {
        return Err(format!("Journal changed during {name} capture"));
    }
    if compose::snapshot(&state.peek(), model) != before {
        return Err(format!(
            "Original storage state changed during {name} capture"
        ));
    }
    rows.push(
        json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,"capture":capture}),
    );
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    let initial = compose::snapshot(&state.peek(), &model);
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(initial["initialDocument"].clone())
            .map_err(|_| "Fixture file malformed")?;
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Final review \u{4e2d}\u{1f642}\nOriginal saved draft A.").await?;
    checked(d, "window.__listCompose.bold()").await?;
    wait_dom(
        d,
        "({ok:window.__listCompose.snapshot().editor?.bold===true})",
    )
    .await?;
    let original = native_blocks(d).await?;
    persisted(state, &model, &original).await?;
    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original A was not safely saved before load recovery".into());
    }
    crate::slack_lists::open(state, file.clone());
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let mut proof = json!({"seed":seed,"originalA":original,"newerB":null,"runtimeForgotten":true});
    let mut rows = Vec::new();

    // A real exclusive transaction prevents the product from opening its saved draft.
    let hold = harness::storage_control(state, &readiness, 1, "hold").await?;
    proof["storage"] = json!({"hold":hold,"release":null});
    click(d, "slack-list-comment-write").await?;
    observed(state, &model, |v| {
        idle(v) && draft(v)["error"].is_string() && draft(v)["savedRevision"].is_null()
    })
    .await?;
    capture(state, &model, d, "load-failed", proof.clone(), &mut rows).await?;
    let release = harness::storage_control(state, &readiness, 2, "release").await?;
    proof["storage"]["release"] = release;
    click(d, "slack-list-comment-retry").await?;
    persisted(state, &model, &original).await?;
    capture(state, &model, d, "load-recovered", proof.clone(), &mut rows).await?;

    // New native B cannot overwrite A while the same actual SQLite database is locked.
    let hold = harness::storage_control(state, &readiness, 3, "hold").await?;
    proof["storage"] = json!({"hold":hold,"release":null});
    proof["runtimeForgotten"] = json!(false);
    type_text(
        d,
        "\nUnsaved draft B \u{65e5}\u{1f98a}\nRetain these changes through local recovery.",
    )
    .await?;
    let newer = native_blocks(d).await?;
    if newer == original {
        return Err("Native B did not differ from A".into());
    }
    proof["newerB"] = newer.clone();
    observed(state, &model, |v| {
        draft(v)["blocks"] == newer && draft(v)["saving"] == true
    })
    .await?;
    settled_error(state, &model, false).await?;
    capture(state, &model, d, "save-failed", proof.clone(), &mut rows).await?;
    click(d, "slack-list-comments-back").await?;
    // Observe the attempted departure's real save, then its bounded failure.
    observed(state, &model, |v| draft(v)["saving"] == true).await?;
    settled_error(state, &model, false).await?;
    capture(state, &model, d, "back-refused", proof.clone(), &mut rows).await?;
    let release = harness::storage_control(state, &readiness, 4, "release").await?;
    proof["storage"]["release"] = release;
    click(d, "slack-list-comment-retry").await?;
    persisted(state, &model, &newer).await?;
    capture(state, &model, d, "save-recovered", proof.clone(), &mut rows).await?;
    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Recovered B was not safely saved before reopening".into());
    }
    reopen(state, d, &file).await?;
    persisted(state, &model, &newer).await?;
    proof["runtimeForgotten"] = json!(true);
    capture(state, &model, d, "save-reopened", proof.clone(), &mut rows).await?;

    // The response exists, but its local settlement must fail until explicit Retry.
    compose::mode(&model, Mode::HeldSuccess)?;
    click(d, "slack-list-comment-send").await?;
    observed(state, &model, |v| {
        counts(v, 1, 1) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    let hold = harness::storage_control(state, &readiness, 5, "hold").await?;
    proof["storage"] = json!({"hold":hold,"release":null});
    proof["runtimeForgotten"] = json!(false);
    proof["postRelease"] = compose::release_post(&model)?;
    settled_error(state, &model, true).await?;
    capture(state, &model, d, "receipt-failed", proof.clone(), &mut rows).await?;
    let release = harness::storage_control(state, &readiness, 6, "release").await?;
    proof["storage"]["release"] = release;
    click(d, "slack-list-comment-retry").await?;
    persisted(state, &model, &json!([])).await?;
    capture(state, &model, d, "receipt-recovered", proof, &mut rows).await?;
    Ok(
        json!({"ok":true,"case":"storage-recovery","fixture":readiness["fixture"],"readiness":readiness,
        "stages":rows,"final":compose::snapshot(&state.peek(),&model),
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
