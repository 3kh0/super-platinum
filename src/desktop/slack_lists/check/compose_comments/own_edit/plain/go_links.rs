//! Original literal go-link input and independent plain Save custody.
use super::*;

fn script(action: &str, arg: Value) -> String {
    format!(
        "({})({},{})",
        include_str!("go_links.js"),
        json!(action),
        arg
    )
}
async fn event(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &script(action, arg)).await
}
fn ops(text: &str) -> Value {
    json!([{"insert":format!("{text}\n")}])
}
async fn settle(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
) -> Result<Value, String> {
    let expected = ops(text);
    wait(state, m, |v| {
        v["capture"]["canSave"] == true
            && v["edit"]["active"]["plainDelta"]["ops"] == expected
            && v["edit"]["active"]["composing"] == false
            && v["edit"]["active"]["nativePending"] == false
    })
    .await?;
    let native = wait_dom(d, &script("state", json!({"ops":expected}))).await?;
    let inventory = durable(state, &expected).await?;
    Ok(json!({"native":native,"inventory":inventory}))
}
async fn key(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    text: &str,
) -> Result<Value, String> {
    let observed = event(d, "key", json!({"key":name})).await?;
    let settled = settle(state, m, d, text).await?;
    Ok(json!({"event":observed,"settled":settled}))
}
async fn insert(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    text: &str,
    expected: &str,
) -> Result<Value, String> {
    let observed = event(d, "insert", json!({"text":text,"composing":false})).await?;
    let settled = settle(state, m, d, expected).await?;
    Ok(json!({"event":observed,"settled":settled}))
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
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, source::go_links::TEXT).await?;
    let space = key(state, &m, d, "Space", "Visit go/team ").await?;
    let newline = key(state, &m, d, "ShiftEnter", "Visit go/team \n").await?;
    let started = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let history_pause = started.elapsed().as_millis();
    let continuation = insert(state, &m, d, "go/team", "Visit go/team \ngo/team").await?;
    let undo = key(state, &m, d, "Undo", "Visit go/team \n").await?;
    let redo = key(state, &m, d, "Redo", "Visit go/team \ngo/team").await?;
    let reset = probe(d, "replace", json!({"text":source::go_links::TEXT})).await?;
    let reset_settled = settle(state, &m, d, source::go_links::TEXT).await?;
    p["goLinks"] = json!({"space":space,"newline":newline,"historyPauseMillis":history_pause,"continuation":continuation,"undo":undo,"redo":redo,"reset":reset,"resetSettled":reset_settled});
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    p["escape"] = probe(d, "key", json!({"key":"Escape"})).await?;
    closed(state, &m, d).await?;
    take(state, &m, d, "cancelled", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, source::go_links::TEXT).await?;
    settle(state, &m, d, source::go_links::TEXT).await?;
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
        return Err("Independent authored go-link readback missing".into());
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    take(state, &m, d, "readback", &p, &mut rows, false).await?;
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":"own-edit-plain-go-links","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Synthetic DOM key/default input, not OS keyboard or full IME.","Only simple go/team form; no blanket generic URL/email detector claim.","Normal history grouping uses an elapsed pause, no fixture history cutoff or semantic Delta injection."]}),
    )
}
