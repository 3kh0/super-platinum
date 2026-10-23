//! Native toolbar fragmentation and retained-formatting positive control.
use super::*;

fn native(action: &str) -> String {
    format!("({})({})", include_str!("runs.js"), json!(action))
}
async fn formatted(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    previous: &Value,
    italic: bool,
) -> Result<Value, String> {
    let model = wait(state, m, |v| {
        let b = &v["edit"]["active"]["blocks"];
        b != previous
            && b[0]["elements"][0]["elements"]
                .as_array()
                .is_some_and(|leaves| leaves.iter().any(|l| l["style"]["italic"] == true) == italic)
            && v["capture"]["canSave"] == true
    })
    .await?;
    let blocks = model["edit"]["active"]["blocks"].clone();
    let proof = checked(d, &native("snapshot")).await?;
    if proof["native"]["blocks"] != blocks {
        return Err("Actual formatted DOM and original capture differ".into());
    }
    inventory(state, &blocks).await?;
    Ok(proof)
}
fn split_only(original: &Value, reverted: &Value) -> Result<(), String> {
    let old = &original[0]["elements"][0]["elements"];
    let new = reverted[0]["elements"][0]["elements"]
        .as_array()
        .ok_or("Missing actual reverted leaves")?;
    if old.as_array().map(Vec::len) != Some(1) || new.len() <= 1 || original == reverted {
        return Err("Native formatting did not produce genuinely distinct run segmentation".into());
    }
    let mut text = String::new();
    for leaf in new {
        let map = leaf.as_object().ok_or("Non-object reverted text")?;
        let part = leaf["text"].as_str().ok_or("Reverted text missing")?;
        if leaf["type"] != "text"
            || leaf["style"] != old[0]["style"]
            || part.is_empty()
            || part.contains(['\r', '\n'])
            || map
                .keys()
                .any(|k| !matches!(k.as_str(), "type" | "text" | "style"))
        {
            return Err("Reverted runs changed ordinary text styles or crossed a barrier".into());
        }
        text.push_str(part);
    }
    let mut normalized = reverted.clone();
    normalized[0]["elements"][0]["elements"] = old.clone();
    if text != old[0]["text"].as_str().ok_or("Original text missing")? || normalized != *original {
        return Err("Formatting changed another original rich field or section".into());
    }
    Ok(())
}
async fn changed_take(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, m).await?;
    let geometry = wait_dom(d, &script("state", json!({"ts":data::RECEIVED_TS}))).await?;
    let journal = compose::journal_snapshot(state).await?;
    let saved = disk::snapshot(state).await?;
    let before = snapshot(state, m);
    if geometry["reply"]["sameNode"] != true
        || geometry["reply"]["blocks"] != p["replyBlocks"]
        || journal["blocks"] != p["replyBlocks"]
        || before["fixture"]["updates"].as_array().map(Vec::len) != Some(1)
    {
        return Err("Changed control lost independent reply or unique update".into());
    }
    let raw = before["fixture"]["responses"][0]["raw"].clone();
    let mut expected = p["changed"]["native"]["blocks"]
        .as_array()
        .ok_or("Changed blocks missing")?
        .clone();
    expected.extend(
        p["source"]["received"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["type"] == "context")
            .cloned(),
    );
    let message = &raw["message"];
    if raw["ok"] != true
        || message["blocks"] != json!(expected)
        || [
            "ts",
            "thread_ts",
            "user",
            "reactions",
            "edit_fixture_opaque",
        ]
        .iter()
        .any(|k| message[*k] != p["source"]["received"][*k])
    {
        return Err("Changed ACK lost original suffix or source metadata".into());
    }
    if name == "readback"
        && !before["comments"]["comments"]["page"]["messages"]
            .as_array()
            .is_some_and(|a| a.iter().any(|r| r == message))
    {
        return Err("Changed formatting missing from independent readback".into());
    }
    let png = harness::capture(d, name).await?;
    if before != snapshot(state, m)
        || journal != compose::journal_snapshot(state).await?
        || saved != disk::snapshot(state).await?
    {
        return Err("Changed control custody changed across capture".into());
    }
    rows.push(json!({"stage":name,"geometry":geometry,"model":before,"journal":journal,"editInventory":saved,"proof":p,"capture":png}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    checked(d, include_str!("../../../comments.js")).await?;
    checked(d, include_str!("../../../compose_comments.js")).await?;
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
    open_edit(state, &m, d, &mut p).await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    p["formatting"] = json!({"selection":checked(d,&native("select")).await?});
    p["formatting"]["onClick"] = checked(d, &native("italic")).await?;
    p["formatting"]["on"] = formatted(state, &m, d, &p["originalBlocks"], true).await?;
    take(state, &m, d, "formatted", &p, &mut rows, false).await?;
    p["formatting"]["offClick"] = checked(d, &native("italic")).await?;
    p["formatting"]["off"] = formatted(
        state,
        &m,
        d,
        &p["formatting"]["on"]["native"]["blocks"],
        false,
    )
    .await?;
    split_only(
        &p["originalBlocks"],
        &p["formatting"]["off"]["native"]["blocks"],
    )?;
    take(state, &m, d, "reverted", &p, &mut rows, false).await?;
    p["saveBefore"] = snapshot(state, &m);
    action(d, &mut p, "slack-list-comment-edit-save").await?;
    closed(state, &m, d).await?;
    p["saveAfter"] = snapshot(state, &m);
    no_preflight(&p["saveBefore"], &p["saveAfter"])?;
    take(state, &m, d, "returned", &p, &mut rows, false).await?;
    open_edit(state, &m, d, &mut p).await?;
    p["changedSelection"] = checked(d, &native("select")).await?;
    p["changedClick"] = checked(d, &native("italic")).await?;
    p["changed"] = formatted(state, &m, d, &p["originalBlocks"], true).await?;
    take(state, &m, d, "changed-edit", &p, &mut rows, false).await?;
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    action(d, &mut p, "slack-list-comment-edit-save").await?;
    wait_fixture(state, |v| v["held"] == true).await?;
    p["doubleSave"] = command(d, "double-save", Value::Null).await?;
    changed_take(state, &m, d, "saving", &p, &mut rows).await?;
    p["release"] = data::release(&state.peek())?;
    editor(d, false).await?;
    wait(state, &m, |v| v["edit"]["active"].is_null()).await?;
    changed_take(state, &m, d, "saved", &p, &mut rows).await?;
    data::publish(&state.peek())?;
    let completed = snapshot(state, &m)["comments"]["ledger"]["completed"]
        .as_u64()
        .ok_or("Missing read count")?;
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
    changed_take(state, &m, d, "readback", &p, &mut rows).await?;
    let expected = [
        "draft",
        "editing",
        "formatted",
        "reverted",
        "returned",
        "changed-edit",
        "saving",
        "saved",
        "readback",
    ];
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("Runs stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":"own-edit-runs","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Actual toolbar and DOM segmentation; no programmatic block replacement.","Only adjacent ordinary same-style text in one section; not full Delta equivalence.","Private same-process SQLite and closed fixture ACK, no live Slack."]}),
    )
}
