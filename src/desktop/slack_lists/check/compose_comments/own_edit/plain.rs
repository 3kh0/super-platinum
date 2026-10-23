//! Named original Quill input, ordered preflight, and independent rich reply.
use super::*;
use fixture::{ordered as wire, plain as source};
mod channel;
mod custom;
mod emoji_display;
mod emoticon;
mod go_links;
mod group;
mod member;
mod member_agents;
mod mixed;
mod noop;
mod smart_symbols;

fn native(action: &str, arg: Value) -> String {
    format!(
        "({})({},{},({}))",
        include_str!("plain.js"),
        json!(action),
        arg,
        include_str!("../reaction_actions/text.js")
    )
}
async fn probe(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &native(action, arg)).await
}
async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..500 {
        let v = snapshot(state, m);
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Original plain edit did not settle: {}",
        snapshot(state, m)
    ))
}
async fn closed(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    wait(state, m, |v| v["edit"]["active"].is_null()).await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-edit-draft')&&document.getElementById('slack-list-comment-draft')?.isContentEditable===true})").await?;
    Ok(())
}
async fn durable(state: Signal<ShellState>, ops: &Value) -> Result<Value, String> {
    for _ in 0..250 {
        let v = data::restart::snapshot(state).await?;
        if v["plain_delta"]["ops"] == *ops && !v["seed"].is_null() && v["attempt"].is_null() {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original plain Delta did not become durable".into())
}
async fn open_plain(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
) -> Result<(), String> {
    let action = command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    let current = wait(state, m, |v| {
        v["capture"]["canSave"] == true
            && v["edit"]["active"]["nativeReady"] == true
            && !v["edit"]["active"]["plainDelta"].is_null()
    })
    .await?;
    let mount = wait_dom(d, &native("remember-edit", Value::Null)).await?;
    if mount["delta"]["ops"] != current["edit"]["active"]["plainDelta"]["ops"]
        || current["edit"]["active"]["source"] != p["source"]["received"]
    {
        return Err("Original plain initializer or source changed".into());
    }
    let inventory = durable(state, &mount["delta"]["ops"]).await?;
    p["opens"].as_array_mut().unwrap().push(json!({"action":action,"active":current["edit"]["active"],"mount":mount,"inventory":inventory}));
    Ok(())
}
async fn replace(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    text: &str,
) -> Result<(), String> {
    let input = probe(d, "replace", json!({"text":text})).await?;
    let ops = json!([{"insert":format!("{text}\n")}]);
    if input["after"]["ops"] != ops {
        return Err(format!("Actual native replacement differs: {}", input));
    }
    wait(state, m, |v| {
        v["edit"]["active"]["plainDelta"]["ops"] == ops && v["capture"]["canSave"] == true
    })
    .await?;
    let inventory = durable(state, &ops).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"native":input,"inventory":inventory}));
    Ok(())
}
async fn take(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    locked: bool,
) -> Result<(), String> {
    if !locked {
        stable_drafts(state, m).await?;
    }
    let mut script = native("state", json!({"ts":data::RECEIVED_TS}));
    if (fixture::plain_channels::named() || fixture::plain_members::named())
        && matches!(name, "save-refused" | "source-ready")
        || source::emoji() && name == "unavailable"
        || fixture::completion::plain_custom_emoji::named() && name == "catalog-failed"
    {
        script = format!(
            "(()=>{{const v=({script});v.ok=v.ok&&v.notice?.text===\"The edit was not sent. Permissions, required metadata, or the original comment changed. Both drafts are retained.\";return v;}})()"
        );
    }
    let geometry = wait_dom(d, &script).await?;
    let journal = if locked {
        Value::Null
    } else {
        compose::journal_snapshot(state).await?
    };
    let inventory = if locked {
        Value::Null
    } else {
        data::restart::snapshot(state).await?
    };
    let before = snapshot(state, m);
    let ordered = wire::snapshot(&state.peek(), m);
    let plain = source::snapshot(&state.peek());
    if geometry["reply"]["sameNode"] != true
        || geometry["reply"]["blocks"] != p["replyBlocks"]
        || !locked && journal["blocks"] != p["replyBlocks"]
    {
        return Err("Plain edit replaced independent rich reply".into());
    }
    let capture = harness::capture(d, name).await?;
    if before != snapshot(state, m)
        || ordered != wire::snapshot(&state.peek(), m)
        || plain != source::snapshot(&state.peek())
        || !locked
            && (journal != compose::journal_snapshot(state).await?
                || inventory != data::restart::snapshot(state).await?)
    {
        return Err(format!("Original plain capture changed at {name}"));
    }
    rows.push(json!({"stage":name,"geometry":geometry,"model":before,"ordered":ordered,"plain":plain,"journal":journal,"editInventory":inventory,"proof":p,"capture":capture}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if fixture::plain_mixed::named() {
        return mixed::run(state, d).await;
    }
    if fixture::plain_groups::named() {
        return group::run(state, d).await;
    }
    if fixture::plain_member_agents::named() {
        return member_agents::run(state, d).await;
    }
    if fixture::plain_members::named() {
        return member::run(state, d).await;
    }
    if fixture::plain_channels::named() {
        return channel::run(state, d).await;
    }
    if source::emoji_display::named() {
        return emoji_display::run(state, d).await;
    }
    if source::go_links::named() {
        return go_links::run(state, d).await;
    }
    if source::smart_symbols::named() {
        return smart_symbols::run(state, d).await;
    }
    if source::emoticon::named() {
        return emoticon::run(state, d).await;
    }
    if source::noop() {
        return noop::run(state, d).await;
    }
    if fixture::completion::plain_custom_emoji::named() {
        return custom::run(state, d).await;
    }
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let seed_source = data::seed(state)?;
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
    let disk = wire::Disk::capture(&state.peek())?;
    let mut p = json!({"source":seed_source,"replySeed":seed,"replyBlocks":remembered["blocks"],"opens":[],"inputs":[],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?});
    let mut rows = Vec::new();
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, source::text()).await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    if source::storage() {
        let hold = harness::storage_control(state, &readiness, 1, "hold").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(hold.clone());
        p["storage"] = json!({"hold":hold,"release":null});
        p["cancelBefore"] = snapshot(state, &m);
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comment-edit-cancel").await?);
        wait(state, &m, |v| {
            v["capture"]["finishing"] == true
                && v["capture"]["canCancel"] == true
                && v["capture"]["busy"] == false
        })
        .await?;
        p["cancelAfter"] = snapshot(state, &m);
        take(state, &m, d, "return-failed", &p, &mut rows, true).await?;
        let release = harness::storage_control(state, &readiness, 2, "release").await?;
        p["storageControls"]
            .as_array_mut()
            .unwrap()
            .push(release.clone());
        p["storage"]["release"] = release;
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comment-edit-cancel").await?);
        closed(state, &m, d).await?;
        take(state, &m, d, "returned", &p, &mut rows, false).await?;
    } else {
        p["escape"] = probe(d, "key", json!({"key":"Escape"})).await?;
        closed(state, &m, d).await?;
        take(state, &m, d, "cancelled", &p, &mut rows, false).await?;
        open_plain(state, &m, d, &mut p).await?;
        if source::emoji() {
            replace(state, &m, d, &mut p, ":custom_unacquired:").await?;
            p["unavailableBefore"] = snapshot(state, &m);
            p["unknownSave"] = button(d, "slack-list-comment-edit-save").await?;
            wait(state, &m, |v| v["edit"]["active"]["notice"] == "The edit was not sent. Permissions, required metadata, or the original comment changed. Both drafts are retained."
                && v["capture"]["canSave"] == true && v["edit"]["attempts"] == json!([])).await?;
            p["unavailableAfter"] = snapshot(state, &m);
            take(state, &m, d, "unavailable", &p, &mut rows, false).await?;
        }
        replace(state, &m, d, &mut p, source::text()).await?;
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        p["enter"] = probe(d, "key", json!({"key":"Enter"})).await?;
        wait_fixture(state, |v| v["held"] == true).await?;
        p["doubleSave"] = command(d, "double-save", Value::Null).await?;
        take(state, &m, d, "saving", &p, &mut rows, false).await?;
        p["release"] = data::release(&state.peek())?;
        closed(state, &m, d).await?;
        take(state, &m, d, "saved", &p, &mut rows, false).await?;
        wire::publish(&state.peek())?;
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comments-refresh").await?);
        wait(state, &m, |v| {
            v["comments"]["comments"]["pageMessages"]
                .as_array()
                .is_some_and(|a| {
                    a.iter()
                        .any(|r| r["ts"] == data::RECEIVED_TS && r["text"] == source::text())
                })
        })
        .await?;
        for _ in 0..400 {
            if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
                == wire::future_message()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
            != wire::future_message()
        {
            return Err("Independent authored plain readback missing".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &m, d, "readback", &p, &mut rows, false).await?;
    }
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if source::storage(){"own-edit-plain-storage"}else if source::emoji(){"own-edit-plain-emoji"}else{"own-edit-plain"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Actual plain Quill input with synthetic native events, not OS keyboard automation.",if source::emoji(){"Standard named emoji and literal Unicode text; unknown custom name refuses. No custom catalog acquisition or plain no-op claim."}else{"Only ordinary changed text; semantic lookup and plain no-op branches are not exercised."},if source::emoji(){"Private SQLite custody and independent rich reply retained; no storage-fault or catalog reacquisition claim."}else{"Private SQLite failure and original rich reply retained in the same process."}]}),
    )
}
