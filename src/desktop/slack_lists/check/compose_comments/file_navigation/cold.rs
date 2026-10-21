//! Cold received tokens acquire through the production coordinator, without a search.
use super::*;
use crate::slack_lists::comments::fixture::cold_files as data_cold;
mod recovery;

const SUCCESS: &[&str] = &[
    "cold-held",
    "cold-loaded",
    "canvas-open",
    "canvas-return",
    "list-open",
    "list-return",
];
const RECOVERY: &[&str] = &[
    "cold-failed",
    "retry-held",
    "source-retired",
    "cold-recovered",
    "visit-held",
    "visit-retired",
];
const RENAMED: &str = "Renamed received Canvas";

fn script(action: &str, arg: &Value) -> String {
    format!(
        "({})({},{},({}))",
        include_str!("cold.js"),
        json!(action),
        arg,
        include_str!("cold_text.js")
    )
}
fn token(id: &str, state: &str, label: &str, retry: bool) -> Value {
    json!({"id":id,"state":state,"label":label,"retry":retry})
}
fn expected(p: &mut Value, canvas: (&str, &str, bool), list: (&str, &str, bool)) {
    p["coldExpected"] = json!({"ts":nav::RECEIVED_TS,"tokens":[
        token(nav::CANVAS,canvas.0,canvas.1,canvas.2),
        token(nav::TARGET_LIST,list.0,list.1,list.2)]});
}
pub(super) async fn geometry(
    d: &dioxus::desktop::DesktopContext,
    p: &Value,
) -> Result<Value, String> {
    if p["coldExpected"].is_null() {
        return Ok(Value::Null);
    }
    wait_dom(d, &script("state", &p["coldExpected"])).await
}
async fn wait_cold(
    state: Signal<ShellState>,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..400 {
        let value = data_cold::snapshot(&state.peek());
        if predicate(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Cold file request lifecycle did not settle".into())
}
async fn held(state: Signal<ShellState>, id: &str) -> Result<usize, String> {
    let v = wait_cold(state, |v| {
        v["held"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|r| r["id"] == id && r["receiverClosed"] == false)
        })
    })
    .await?;
    v["held"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id && r["receiverClosed"] == false)
        .and_then(|r| r["serial"].as_u64())
        .map(|s| s as usize)
        .ok_or("Held serial missing".into())
}
async fn release(
    state: Signal<ShellState>,
    serial: usize,
    delivered: bool,
    p: &mut Value,
) -> Result<(), String> {
    let proof = data_cold::release(&state.peek(), serial).map_err(|e| e.to_string())?;
    if proof["delivered"] != delivered || proof["cancelledBeforeReply"] == delivered {
        return Err("Cold read delivery/cancellation mismatch".into());
    }
    p["releases"].as_array_mut().unwrap().push(proof);
    Ok(())
}
async fn retry(d: &dioxus::desktop::DesktopContext, p: &mut Value, id: &str) -> Result<(), String> {
    let proof = checked(d, &script("retry", &json!({"ts":nav::RECEIVED_TS,"id":id}))).await?;
    p["retries"].as_array_mut().unwrap().push(proof);
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Workspace missing")?
        .rt_generation = 7;
    data::seed(state).await?;
    let cold_seed = data_cold::seed(&mut state.write())?;
    if case == "cold-files-recovery" {
        data_cold::configure(&state.peek(), nav::CANVAS, data_cold::Mode::Offline)
            .map_err(|e| e.to_string())?;
        data_cold::configure(&state.peek(), nav::TARGET_LIST, data_cold::Mode::Malformed)
            .map_err(|e| e.to_string())?;
    }
    checked(d, include_str!("../../comments.js")).await?;
    checked(d, include_str!("../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    let disk = Disk::capture(&state.peek(), &seed)?;
    write(d).await?;
    type_text(d, "Final review \u{4e2d}\u{1f642}").await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    let mut p = json!({"seed":seed,"coldSeed":cold_seed,"received":cold_seed["navigation"],"expectedBlocks":group_completion::current_blocks(d).await?,
        "clicks":[],"retries":[],"releases":[],"runtimeForgotten":false,"coldExpected":null});
    stable_drafts(state, &m).await?;
    let mut rows = Vec::new();
    if case == "cold-files" {
        let first = held(state, nav::CANVAS).await?;
        expected(
            &mut p,
            ("loading", "Original Canvas fallback", false),
            ("loading", "Original List fallback", false),
        );
        capture(state, &m, d, &disk, "cold-held", &p, &mut rows, None, false).await?;
        release(state, first, true, &mut p).await?;
        let second = held(state, nav::TARGET_LIST).await?;
        release(state, second, true, &mut p).await?;
        expected(
            &mut p,
            ("ready", "Alex Canvas", false),
            ("ready", "Alex List", false),
        );
        capture(
            state,
            &m,
            d,
            &disk,
            "cold-loaded",
            &p,
            &mut rows,
            None,
            false,
        )
        .await?;
        for (id, edit, opened, returned) in [
            (nav::CANVAS, CANVAS_EDIT, "canvas-open", "canvas-return"),
            (nav::TARGET_LIST, LIST_EDIT, "list-open", "list-return"),
        ] {
            p["runtimeForgotten"] = json!(false);
            click_file(d, &mut p, id, Some(edit)).await?;
            let prior = p["coldExpected"].take();
            capture(state, &m, d, &disk, opened, &p, &mut rows, Some(id), false).await?;
            restored(state, &m, d, &mut p).await?;
            p["coldExpected"] = prior;
            capture(state, &m, d, &disk, returned, &p, &mut rows, None, false).await?;
        }
    } else {
        recovery::run(state, &m, d, &disk, &mut p, &mut rows).await?;
    }
    let stages = if case == "cold-files" {
        SUCCESS
    } else {
        RECOVERY
    };
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != stages
    {
        return Err("Cold stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m),
        "initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,
        "limitations":["Closed singleton files.info is a native adaptation; full Slack transcript batch scheduling is not claimed.","Local source invalidation and actual native Back exercise retirement; other account/body/socket cases are focused Rust regressions."]}),
    )
}
