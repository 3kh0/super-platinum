//! Existing visit replacement, durable restore, failed file read and pre-mount Cancel.
use super::*;
use crate::slack_lists::comments::fixture::cold_files as cold;

fn transcript_row(value: &Value) -> Option<&Value> {
    let mut rows = value["rows"]
        .as_array()?
        .iter()
        .filter(|r| r["id"] == source_file());
    let row = rows.next()?;
    rows.next().is_none().then_some(row)
}

async fn transcript_recovered(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    original: &Value,
    requests: &Value,
) -> Result<Value, String> {
    let old = transcript_row(original).ok_or("Original transcript demand missing or ambiguous")?;
    let settled = wait(state, m, |v| {
        let titles = &v["transcriptTitles"];
        titles["running"] == false
            && titles["generation"] == original["generation"]
            && transcript_row(titles).is_some_and(|r| {
                r["serial"] != old["serial"]
                    && r["ordinal"] == old["ordinal"]
                    && r["phase"] == "ready"
                    && r["state"] == "ready"
                    && r["current"] == true
                    && r["titleUsers"]["current"] == true
            })
    })
    .await?;
    let expected = json!({"ts":data::RECEIVED_TS,"files":[
        {"id":source_file(),"kind":"canvas","label":source::label()},
        {"id":source_file(),"kind":"canvas","label":source::label()}]});
    let native = wait_dom(
        d,
        &format!(
            "({})({},true)",
            include_str!("../../file_completion/transcript.js"),
            expected
        ),
    )
    .await?;
    let after = snapshot(state, m);
    if after["transcriptTitles"] != settled["transcriptTitles"]
        || after["coldFiles"]["requests"] != *requests
        || requests.as_array().map(Vec::len) != Some(4)
    {
        return Err(
            "Recovered transcript changed during DOM proof or acquired extra metadata".into(),
        );
    }
    Ok(json!({"after":after["transcriptTitles"],"native":native,
        "readLedgerAfter":after["coldFiles"]["requests"]}))
}

async fn held_file(state: Signal<ShellState>, m: &Model) -> Result<usize, String> {
    let v = wait(state, m, |v| {
        v["coldFiles"]["held"].as_array().is_some_and(|r| {
            r.len() == 1 && r[0]["id"] == source_file() && r[0]["receiverClosed"] == false
        })
    })
    .await?;
    v["coldFiles"]["held"][0]["serial"]
        .as_u64()
        .map(|n| n as usize)
        .ok_or("Original files.info serial missing".into())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    // Same accepted synthetic visit replacement and detached native capture as
    // the member-title case. No edit/session setter and no fabricated ready state.
    super::held::prepare(state, m, d, p, rows).await?;
    wait(state, m, |v| {
        v["transcriptTitles"]["rows"]
            .as_array()
            .is_some_and(|rows| {
                let relevant = rows
                    .iter()
                    .filter(|r| r["id"] == source_file())
                    .collect::<Vec<_>>();
                !relevant.is_empty() && relevant.iter().all(|r| r["titleUsers"]["current"] == true)
            })
    })
    .await?;
    p["returnTranscript"] = snapshot(state, m)["transcriptTitles"].clone();
    cold::configure(&state.peek(), source_file(), cold::Mode::Offline)
        .map_err(|e| e.to_string())?;
    p["fileInvalidation"] =
        fixture::completion::files::invalidate(&mut state.write(), source_file())
            .map_err(|e| e.to_string())?;
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    wait(state, m, |v| {
        v["edit"]["active"]["titleUsers"]["status"] == "failed"
            && v["edit"]["active"]["titleUsers"]["current"] == true
    })
    .await?;
    if snapshot(state, m)["edit"]["active"]["blocks"] != p["editedBlocks"] {
        return Err("Restored title lookup lost durable modified edit".into());
    }
    p["restoredOriginal"] = snapshot(state, m)["edit"]["active"].clone();
    pending(state, m, d, "restored-failed", p, rows).await?;
    cold::configure(&state.peek(), source_file(), cold::Mode::Held).map_err(|e| e.to_string())?;
    action(d, p, "slack-list-comment-edit-title-retry").await?;
    let old = held_file(state, m).await?;
    p["heldOriginal"] = snapshot(state, m)["edit"]["active"].clone();
    p["heldFile"] = json!({"serial":old,"source":snapshot(state,m)["coldFiles"]["source"]});
    pending(state, m, d, "retry-held", p, rows).await?;
    action(d, p, "slack-list-comment-edit-cancel").await?;
    wait(state, m, |v| {
        v["edit"]["active"].is_null()
            && v["coldFiles"]["held"].as_array().is_some_and(|a| {
                a.len() == 1 && a[0]["serial"] == old && a[0]["receiverClosed"] == true
            })
    })
    .await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-edit')&&document.getElementById('slack-list-comment-draft')?.isContentEditable})").await?;
    if !data::restart::snapshot(state).await?["seed"].is_null() {
        return Err("Cancel retained the deliberately discarded edit".into());
    }
    capture(state, m, d, "cancelled", p, rows).await?;
    let released = cold::release(&state.peek(), old).map_err(|e| e.to_string())?;
    if released["delivered"] != false || released["cancelledBeforeReply"] != true {
        return Err("Cancelled edit accepted old file reply".into());
    }
    p["releases"].as_array_mut().unwrap().push(released);
    wait(state, m, |v| {
        v["coldFiles"]["held"] == json!([]) && v["edit"]["active"].is_null()
    })
    .await?;
    if snapshot(state, m)["coldFiles"]["source"] != p["heldFile"]["source"]
        || !data::restart::snapshot(state).await?["seed"].is_null()
    {
        return Err("Late title callback changed source or cancelled edit journal".into());
    }
    capture(state, m, d, "late", p, rows).await?;
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    let fresh = held_file(state, m).await?;
    let active = snapshot(state, m)["edit"]["active"].clone();
    if fresh == old
        || active["titleUsers"]["scope"] == p["heldOriginal"]["titleUsers"]["scope"]
        || active["blocks"] == p["editedBlocks"]
    {
        return Err("Fresh Edit reused cancelled modified seed or title owner".into());
    }
    p["freshFile"] = json!({"serial":fresh,"job":active["titleUsers"]});
    pending(state, m, d, "fresh-held", p, rows).await?;
    let before = snapshot(state, m);
    let stale = &before["transcriptTitles"];
    if !transcript_row(stale).is_some_and(|r| {
        r["phase"] == "ready"
            && r["current"] == true
            && r["state"] == "unavailable"
            && r["titleUsers"]["current"] == false
    }) {
        return Err(
            "Expected retained stale Ready transcript before positive file recovery".into(),
        );
    }
    p["transcriptRecovery"] = json!({"before":stale,
        "readLedgerBefore":before["coldFiles"]["requests"]});
    let released = cold::release(&state.peek(), fresh).map_err(|e| e.to_string())?;
    if released["delivered"] != true {
        return Err("Fresh edit file reply was retired".into());
    }
    p["releases"].as_array_mut().unwrap().push(released);
    editor(d, true).await?;
    ready(state, m, d, p).await?;
    p["transcriptRecovery"]["editorMount"] = command(d, "remember-edit", Value::Null).await?;
    let native_before = command(d, "state", json!({"ts":data::RECEIVED_TS})).await?;
    let recovered = transcript_recovered(
        state,
        m,
        d,
        &p["transcriptRecovery"]["before"],
        &p["transcriptRecovery"]["readLedgerBefore"],
    )
    .await?;
    let native_after = command(d, "state", json!({"ts":data::RECEIVED_TS})).await?;
    if native_after["edit"]["sameNode"] != true
        || native_after["reply"]["sameNode"] != true
        || native_after["edit"]["blocks"] != native_before["edit"]["blocks"]
        || native_after["reply"]["blocks"] != native_before["reply"]["blocks"]
    {
        return Err(
            "Independent transcript recovery changed original edit or reply DOM custody".into(),
        );
    }
    for (key, value) in recovered.as_object().unwrap() {
        p["transcriptRecovery"][key] = value.clone();
    }
    p["transcriptRecovery"]["editorBefore"] = native_before;
    p["transcriptRecovery"]["editorAfter"] = native_after;
    p["freshSeed"] = snapshot(state, m)["edit"]["active"].clone();
    durable(state, &p["freshSeed"]["blocks"]).await?;
    capture(state, m, d, "fresh-edit", p, rows).await?;
    action(d, p, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    wait(state, m, |v| v["edit"]["active"].is_null()).await?;
    p["transcriptRecoveryFinal"] = transcript_recovered(
        state,
        m,
        d,
        &p["transcriptRecovery"]["before"],
        &p["transcriptRecovery"]["readLedgerBefore"],
    )
    .await?;
    if p["transcriptRecoveryFinal"]["after"] != p["transcriptRecovery"]["after"] {
        return Err("Cancel replaced the independently recovered transcript receipt".into());
    }
    capture(state, m, d, "final-cancel", p, rows).await
}
