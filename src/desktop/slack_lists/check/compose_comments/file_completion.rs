//! Closed List file suggestions through original mixed query, native caret and journal.
use super::group_completion::{
    capture, capture_argument, choose, command, current_blocks, geometry, query, reload, snapshot,
    wait,
};
use super::*;
use crate::slack_lists::comments::fixture::completion as data;
mod custody;
mod source;
mod success;
fn searches(v: &Value) -> usize {
    v["completionSource"]["requests"].as_array().map_or(0, |r| {
        r.iter()
            .filter(|r| r["method"] == "search.autocomplete.files")
            .count()
    })
}
fn search_ready(v: &Value, n: usize, completed: usize, cancelled: usize, held: usize) -> bool {
    searches(v) == n
        && v["completionSource"]["completed"] == completed
        && v["completionSource"]["cancelled"] == cancelled
        && v["completionSource"]["held"] == held
}
async fn windows(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    rows: &mut [Value],
    middle: bool,
) -> Result<(), String> {
    let before = snapshot(state, m);
    let arg = capture_argument(&before, name, false);
    for position in if middle {
        vec!["middle", "bottom"]
    } else {
        vec!["bottom"]
    } {
        if position == "middle" {
            checked(d,"(()=>{const p=document.querySelector('.slack-list-channel-completions');if(!p)return {ok:false};p.scrollTop=(p.scrollHeight-p.clientHeight)/2;return {ok:true};})()").await?;
        } else {
            command(d, "scroll", json!("bottom")).await?;
        }
        let g = geometry(d, &arg).await?;
        let image = harness::capture(d, &format!("{name}-{position}")).await?;
        command(d, "snapshot", arg.clone()).await?;
        if before != snapshot(state, m) {
            return Err("File scroll crossed original publication".into());
        }
        rows.last_mut().ok_or("Missing capture")?["proof"][format!("{position}Window")] =
            json!({"geometry":g,"capture":image,"model":before});
    }
    command(d, "scroll", json!("top")).await?;
    Ok(())
}
async fn off(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, "draft", p, rows, false).await?;
    query(state, m, d, p, "off-results", "@alex", data::Mode::Success).await?;
    wait(state, m, |v| {
        search_ready(v, 0, 0, 0, 0)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["values"].as_array().is_some_and(|a| {
                a.len() == 7
                    && a.iter()
                        .all(|r| matches!(r["value"]["type"].as_str(), Some("user" | "usergroup")))
            })
    })
    .await?;
    custody::settled_status(state, m, d, p, "off-results", false).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "off-results", p, rows, false).await?;
    windows(state, m, d, "off-results", rows, false).await?;
    choose(
        state,
        m,
        d,
        p,
        "member-inserted",
        "@Alex Harbor",
        data::members::SECOND,
        true,
    )
    .await?;
    capture(state, m, d, "member-inserted", p, rows, false).await?;
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
        "Final review \u{4e2d}\u{1f642}\nSecond line <literal> & *punctuation*.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    edits::witness(d).await?;
    let original = current_blocks(d).await?;
    let mut p = json!({"seed":seed,"originalA":original,"expectedBlocks":original,"inputs":[],"releases":[],"runtimeForgotten":false});
    let mut rows = vec![];
    if case == "file-completion" {
        success::run(state, &m, d, &mut p, &mut rows).await?;
    } else if case == "file-completion-off" {
        off(state, &m, d, &mut p, &mut rows).await?;
    } else {
        custody::run(state, &m, d, &mut p, &mut rows).await?;
        source::run(state, &m, d, &mut p, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,
        "limitations":["Named synthetic Context carries no HTTP client; all request factories receive closed replies.","Native DOM keyboard and pointer handlers, not OS IME.","Popup samples prioritize suggestions; after dismissal real editor text is measured."]}),
    )
}
