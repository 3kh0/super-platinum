//! Rich UI only: original ordered HTTP Page custody through a real changed Save.
use super::*;
use fixture::ordered as wire;

async fn wait_ordered(
    state: Signal<ShellState>,
    m: &Model,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..400 {
        let v = wire::snapshot(&state.peek(), m);
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original ordered replies did not settle".into())
}
async fn take(
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
    let inventory = data::restart::snapshot(state).await?;
    let before = snapshot(state, m);
    let ordered = wire::snapshot(&state.peek(), m);
    if geometry["reply"]["sameNode"] != true
        || geometry["reply"]["blocks"] != p["replyBlocks"]
        || journal["blocks"] != p["replyBlocks"]
    {
        return Err("Ordered read changed independent reply custody".into());
    }
    let png = harness::capture(d, name).await?;
    if before != snapshot(state, m)
        || ordered != wire::snapshot(&state.peek(), m)
        || journal != compose::journal_snapshot(state).await?
        || inventory != data::restart::snapshot(state).await?
    {
        return Err(format!("Ordered capture changed at {name}"));
    }
    rows.push(json!({"stage":name,"geometry":geometry,"model":before,"ordered":ordered,"journal":journal,"editInventory":inventory,"proof":p,"capture":png}));
    Ok(())
}
async fn action(
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    id: &str,
) -> Result<(), String> {
    let result = button(d, id).await?;
    p["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":id,"native":result}));
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
    let mut p = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?});
    let mut rows = Vec::new();
    wait_ordered(state, &m, |v| {
        v["pages"].as_array().map(Vec::len) == Some(1)
    })
    .await?;
    take(state, &m, d, "draft", &p, &mut rows).await?;
    p["open"] = command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-edit-save')?.disabled===false})",
    )
    .await?;
    p["mount"] = command(d, "remember-edit", Value::Null).await?;
    wait_ordered(state, &m, |v| {
        v["pages"].as_array().map(Vec::len) == Some(2)
    })
    .await?;
    take(state, &m, d, "editing", &p, &mut rows).await?;
    p["append"] = command(d, "append", json!({"text":" Changed."})).await?;
    p["editedBlocks"] = p["append"]["blocks"].clone();
    let future = wire::future_message();
    let expected: Vec<_> = future["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] != "context")
        .cloned()
        .collect();
    if p["editedBlocks"] != json!(expected) {
        return Err(format!(
            "Actual native changed blocks differ from authored future body: {}",
            p["editedBlocks"]
        ));
    }
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    action(d, &mut p, "slack-list-comment-edit-save").await?;
    wait_fixture(state, |v| v["held"] == true).await?;
    wait_ordered(state, &m, |v| {
        v["pages"].as_array().map(Vec::len) == Some(3)
    })
    .await?;
    p["doubleSave"] = command(d, "double-save", Value::Null).await?;
    take(state, &m, d, "saving", &p, &mut rows).await?;
    p["release"] = data::release(&state.peek())?;
    editor(d, false).await?;
    for _ in 0..400 {
        if editing::snapshot(&state.peek())["active"].is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if !editing::snapshot(&state.peek())["active"].is_null() {
        return Err("Changed edit did not close".into());
    }
    take(state, &m, d, "saved", &p, &mut rows).await?;
    wire::publish(&state.peek())?;
    action(d, &mut p, "slack-list-comments-refresh").await?;
    wait_ordered(state, &m, |v| {
        v["pages"].as_array().map(Vec::len) == Some(4)
            && v["entryPage"]["raw"]["messages"][1] == future
    })
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    take(state, &m, d, "readback", &p, &mut rows).await?;
    wire::hold(&state.peek())?;
    action(d, &mut p, "slack-list-comments-refresh").await?;
    wait_ordered(state, &m, |v| v["held"] == true).await?;
    p["beforeReplacementDisk"] = disk.snapshot().await?;
    take(state, &m, d, "held", &p, &mut rows).await?;
    let (media, epoch, account) = {
        let s = state.peek();
        (
            s.media.clone(),
            s.core.account_epoch,
            s.core.active_account.clone(),
        )
    };
    let mut replacement = ShellState::fixture(media);
    replacement.slack_lists = Default::default();
    replacement.overlay = None;
    replacement.core.active_account = Some("list-ordered-successor".into());
    replacement.core.account_epoch = epoch + 1;
    crate::bootstrap::install_loaded(state, replacement);
    let detached=wait_dom(d,"({ok:!document.querySelector('.slack-list-dialog')&&!document.getElementById('slack-list-comment-edit-draft')&&!document.getElementById('slack-list-comment-draft')})").await?;
    wait_ordered(state, &m, |v| {
        v["completed"] == 5 && v["cancelled"] == 1 && v["heldOwnerCurrent"] == false
    })
    .await?;
    p["replacement"] = json!({"originalAccount":account,"originalEpoch":epoch,"release":wire::release(&m)?,"detached":detached,"syntheticAccountReplacement":true});
    let after_disk = disk.snapshot().await?;
    if after_disk["reply"]["blocks"] != p["replyBlocks"]
        || after_disk["edit"] != p["beforeReplacementDisk"]["edit"]
    {
        return Err("Original private journal changed under replacement".into());
    }
    p["afterReplacementDisk"] = after_disk;
    let ordered = wire::snapshot(&state.peek(), &m);
    if ordered["pages"].as_array().map(Vec::len) != Some(4)
        || !ordered["entryPage"].is_null()
        || ordered["heldOwnerCurrent"] != false
    {
        return Err("Retired original Page was adopted by successor".into());
    }
    let successor = compose::snapshot(&state.peek(), &m);
    let png = harness::capture(d, "replaced").await?;
    if successor != compose::snapshot(&state.peek(), &m)
        || ordered != wire::snapshot(&state.peek(), &m)
    {
        return Err("Successor changed across capture".into());
    }
    rows.push(json!({"stage":"replaced","geometry":detached,"model":successor,"ordered":ordered,"journal":p["afterReplacementDisk"]["reply"],"editInventory":p["afterReplacementDisk"]["edit"],"proof":p,"capture":png}));
    Ok(
        json!({"ok":true,"case":"own-edit-ordered","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":compose::snapshot(&state.peek(),&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Rich editing only; plain-mode support is not enabled or claimed.","Original authored HTTP bytes and actual parsed Page observations, including private preflight.","Explicit synthetic account replacement, not human navigation or process restart.","No native realtime stage; executable core tests cover typed mutation and transport."]}),
    )
}
