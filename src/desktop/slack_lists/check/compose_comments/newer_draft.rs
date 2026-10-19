//! Actual native B survives the independently confirmed, previously issued A.
use super::*;

const STAGES: &[&str] = &[
    "a-unconfirmed",
    "b-edited",
    "b-reopened",
    "a-confirmed-b-retained",
    "b-sent",
];

async fn native_blocks(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let value = wait_dom(d, "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true;return v;})()").await?;
    Ok(value["editor"]["blocks"].clone())
}

async fn saved_blocks(
    state: Signal<ShellState>,
    model: &Model,
    blocks: &Value,
) -> Result<(), String> {
    wait_model(state, model, |v| {
        v["draftStore"]["drafts"]
            .as_array()
            .is_some_and(|rows| rows.len() == 1 && rows[0]["blocks"] == *blocks)
    })
    .await?;
    stable_drafts(state, model).await?;
    let journal = compose::journal_snapshot(state).await?;
    if journal["blocks"] != *blocks {
        return Err("Native draft did not reach the isolated journal".into());
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
        return Err("Newer-draft stage order changed".into());
    }
    stable_drafts(state, model).await?;
    let expression = if name == "b-sent" {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&(!v.editor||v.editor.text.trim()==='');return v;})()"
    } else if name == "a-confirmed-b-retained" {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true&&document.getElementById('slack-list-comment-send')?.disabled===false;return v;})()"
    } else {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true&&document.getElementById('slack-list-comment-send')?.disabled===true;return v;})()"
    };
    let geometry = wait_dom(d, expression).await?;
    proof["journal"] = compose::journal_snapshot(state).await?;
    let before = compose::snapshot(&state.peek(), model);
    if before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
    {
        return Err("Newer-draft fixture acquired live state".into());
    }
    let capture = harness::capture(d, name).await?;
    let journal_after = compose::journal_snapshot(state).await?;
    let after = compose::snapshot(&state.peek(), model);
    if after != before || journal_after != proof["journal"] {
        return Err(format!(
            "Original newer-draft state changed during {name} capture"
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
    let file = serde_json::from_value(initial["initialDocument"].clone())
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
    type_text(
        d,
        "Final review \u{4e2d}\u{1f642}\nOriginal attempt A stays unchanged.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    wait_dom(
        d,
        "({ok:window.__listCompose.snapshot().editor?.bold===true})",
    )
    .await?;
    let original = native_blocks(d).await?;
    saved_blocks(state, &model, &original).await?;
    compose::mode(&model, Mode::HeldAmbiguous)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| {
        counts(v, 1, 1) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    let release = compose::release_post(&model)?;
    wait_dom(d, "({ok:document.getElementById('slack-list-comments')?.textContent.toLowerCase().includes('unconfirmed')===true})").await?;
    stable_drafts(state, &model).await?;
    let mut rows = Vec::new();
    capture(state, &model, d, "a-unconfirmed", json!({
        "originalA":original,"newerB":null,"seed":seed,"runtimeForgotten":false,"release":release
    }), &mut rows).await?;

    // This is native editing while the persisted original attempt remains locked.
    type_text(
        d,
        "\nNewer draft B \u{65e5}\u{1f98a}\nKeep this newer text after confirming A.",
    )
    .await?;
    let newer = native_blocks(d).await?;
    if newer == original {
        return Err("Native B did not differ from original A".into());
    }
    saved_blocks(state, &model, &newer).await?;
    let proof = json!({"originalA":original,"newerB":newer,"seed":seed,"runtimeForgotten":false});
    capture(state, &model, d, "b-edited", proof.clone(), &mut rows).await?;

    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Newer B was not safely persisted before runtime eviction".into());
    }
    reopen(state, d, &file).await?;
    saved_blocks(state, &model, &newer).await?;
    let mut recovered = proof.clone();
    recovered["runtimeForgotten"] = json!(true);
    capture(state, &model, d, "b-reopened", recovered, &mut rows).await?;

    compose::publish(&model)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, &model, |v| {
        v["draftStore"]["drafts"].as_array().is_some_and(|rows| {
            rows.len() == 1 && rows[0]["attempt"].is_null() && rows[0]["blocks"] == newer
        })
    })
    .await?;
    settled(d).await?;
    saved_blocks(state, &model, &newer).await?;
    capture(
        state,
        &model,
        d,
        "a-confirmed-b-retained",
        proof.clone(),
        &mut rows,
    )
    .await?;

    compose::mode(&model, Mode::Success)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| {
        counts(v, 2, 2)
            && v["draftStore"]["drafts"].as_array().is_some_and(|rows| {
                rows.len() == 1
                    && rows[0]["attempt"].is_null()
                    && rows[0]["blocks"].as_array().is_some_and(Vec::is_empty)
            })
    })
    .await?;
    stable_drafts(state, &model).await?;
    capture(state, &model, d, "b-sent", proof, &mut rows).await?;
    Ok(
        json!({"ok":true,"case":"newer-draft","fixture":readiness["fixture"],"readiness":readiness,
        "stages":rows,"final":compose::snapshot(&state.peek(),&model),
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
