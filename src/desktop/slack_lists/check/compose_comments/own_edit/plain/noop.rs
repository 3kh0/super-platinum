//! Actual unchanged and exact native text reversion; original Save and local return.
use super::*;

fn unchanged(value: &Value, source: &Value) -> Result<(), String> {
    let original = &source["received"];
    let installed = value["comments"]["comments"]["pageMessages"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["ts"] == data::RECEIVED_TS));
    if installed.is_none_or(|row| {
        original
            .as_object()
            .is_none_or(|raw| raw.iter().any(|(key, value)| row[key] != *value))
            || row["edited"] != original["edited"]
    }) || value["fixture"]["received"] != *original
        || ["prepared", "updates", "responses", "futureRows", "releases"]
            .iter()
            .any(|k| value["fixture"][*k] != json!([]))
        || value["edit"]["attempts"] != json!([])
    {
        return Err("Plain no-op changed the original row or created update work".into());
    }
    Ok(())
}
async fn shot(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    locked: bool,
) -> Result<(), String> {
    unchanged(&snapshot(state, m), &p["source"])?;
    take(state, m, d, name, p, rows, locked).await
}
async fn click(d: &dioxus::desktop::DesktopContext, p: &mut Value, id: &str) -> Result<(), String> {
    p["actions"]
        .as_array_mut()
        .unwrap()
        .push(button(d, id).await?);
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let seed_source = data::seed(state)?;
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
    let disk = wire::Disk::capture(&state.peek())?;
    let mut p = json!({"source":seed_source,"replySeed":seed,"replyBlocks":remembered["blocks"],"opens":[],"inputs":[],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?});
    let mut rows = Vec::new();
    shot(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    p["originalOps"] = p["opens"][0]["mount"]["delta"]["ops"].clone();
    let original_text = p["originalOps"]
        .as_array()
        .filter(|ops| ops.len() == 1)
        .and_then(|ops| ops[0]["insert"].as_str())
        .and_then(|text| text.strip_suffix('\n'))
        .ok_or("Original plain fixture is not a single native text run")?
        .to_string();
    if p["originalOps"][0] != json!({"insert":format!("{original_text}\n")}) {
        return Err("Original plain run has unobserved attributes".into());
    }
    shot(
        state,
        &m,
        d,
        if source::storage() {
            "editing"
        } else {
            "unchanged-edit"
        },
        &p,
        &mut rows,
        false,
    )
    .await?;
    if source::storage() {
        let hold = harness::storage_control(state, &readiness, 1, "hold").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(hold.clone());
        p["storage"] = json!({"hold":hold,"release":null});
    }
    p["saveBefore"] = snapshot(state, &m);
    click(d, &mut p, "slack-list-comment-edit-save").await?;
    if source::storage() {
        wait(state, &m, |v| {
            v["capture"]["finishing"] == true
                && v["capture"]["canCancel"] == true
                && v["capture"]["busy"] == false
        })
        .await?;
        p["saveAfter"] = snapshot(state, &m);
        shot(state, &m, d, "return-failed", &p, &mut rows, true).await?;
        let release = harness::storage_control(state, &readiness, 2, "release").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(release.clone());
        p["storage"]["release"] = release;
        click(d, &mut p, "slack-list-comment-edit-cancel").await?;
        closed(state, &m, d).await?;
        shot(state, &m, d, "returned", &p, &mut rows, false).await?;
    } else {
        closed(state, &m, d).await?;
        p["saveAfter"] = snapshot(state, &m);
        shot(state, &m, d, "unchanged-return", &p, &mut rows, false).await?;
        open_plain(state, &m, d, &mut p).await?;
        replace(state, &m, d, &mut p, "Temporary plain edit").await?;
        shot(state, &m, d, "modified", &p, &mut rows, false).await?;
        replace(state, &m, d, &mut p, &original_text).await?;
        if snapshot(state, &m)["edit"]["active"]["plainDelta"]["ops"] != p["originalOps"] {
            return Err("Actual original input did not exactly revert".into());
        }
        shot(state, &m, d, "reverted", &p, &mut rows, false).await?;
        p["revertSaveBefore"] = snapshot(state, &m);
        p["enter"] = probe(d, "key", json!({"key":"Enter"})).await?;
        closed(state, &m, d).await?;
        p["revertSaveAfter"] = snapshot(state, &m);
        shot(state, &m, d, "reverted-return", &p, &mut rows, false).await?;
        let previous = snapshot(state, &m)["comments"]["ledger"]["completed"]
            .as_u64()
            .ok_or("Original read count missing")?;
        click(d, &mut p, "slack-list-comments-refresh").await?;
        wait(state, &m, |v| {
            v["comments"]["ledger"]["completed"] == json!(previous + 4)
                && v["comments"]["comments"]["loading"] == false
        })
        .await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        shot(state, &m, d, "readback", &p, &mut rows, false).await?;
        open_plain(state, &m, d, &mut p).await?;
        shot(state, &m, d, "reopened", &p, &mut rows, false).await?;
        p["finalCancel"] = button(d, "slack-list-comment-edit-cancel").await?;
        closed(state, &m, d).await?;
        p["finalReturn"] = json!({"native":probe(d,"state",json!({"ts":data::RECEIVED_TS})).await?,"model":snapshot(state,&m),"disk":disk.snapshot().await?});
    }
    unchanged(&snapshot(state, &m), &p["source"])?;
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if source::storage(){"own-edit-plain-noop-storage"}else{"own-edit-plain-noop"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Original native unchanged Delta and actual text change/exact reversion; fresh Save preflight remains observable.","No update or durable send attempt; local cleanup failure and Retry return preserve independent rich reply.","Synthetic DOM input and same-process private SQLite, not OS keyboard or account replacement coverage."]}),
    )
}
