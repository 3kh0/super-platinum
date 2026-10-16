//! Named disconnected Search filter workflow; never changes the product controls.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::time::Duration;
use super_platinum_core::search_query::SearchFilters;

pub(crate) const QUERY: &str =
    "launch has:file has:file -has:file \"has:link\" has:pin HAS:link has:links";
const APPLIED: &str = "launch   -has:file \"has:link\" has:pin HAS:link has:links has:link";
const CLEARED: &str = "launch   -has:file \"has:link\" has:pin HAS:link has:links ";
const KEYBOARD: &str = "report \"exact phrase\" -has:link has:pin";
const UNFINISHED: &str = "report \"unfinished";
const ERROR: &str = "Close the quoted phrase before adding a filter.";

fn fixture() -> Option<String> {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").ok()?;
    matches!(
        name.as_str(),
        "search-has-filters" | "search-has-filters-narrow"
    )
    .then_some(name)
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.overlay = Some(Overlay::Search);
    shell.replace_search_query(QUERY.into());
    shell.search_filters = Some(SearchFilters::from_query(QUERY));
    // No result/page success is fabricated. Search's actual credential refusal runs.
}
fn isolated(state: &ShellState) -> Result<(), String> {
    if fixture().is_some()
        && crate::fixture::is_fixture()
        && state.core.session.is_none()
        && state.core.transport.is_none()
        && state.overlay == Some(Overlay::Search)
    {
        Ok(())
    } else {
        Err("Search check requires its exact disconnected Search fixture".into())
    }
}
pub(crate) fn start(state: Signal<ShellState>, _desktop: dioxus::desktop::DesktopContext) {
    if fixture().is_none() || std::env::var("SUPER_PLATINUM_SEARCH_HAS_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let value = match tokio::time::timeout(Duration::from_secs(160), run(state)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Search filter check timed out"}),
        };
        eprintln!("search-has-filters-check: {value}");
    });
}
async fn eval(body: String) -> Result<Value, String> {
    let script = format!(
        "try {{const probe={};{} }}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
        include_str!("check_dom.js"),
        body
    );
    let mut evaluation = dioxus::document::eval(&script);
    let response = tokio::time::timeout(Duration::from_secs(5), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match response {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Search DOM: {v}")),
        _ => Err("Search DOM timeout".into()),
    }
}
async fn action(kind: &str, value: &str) -> Result<Value, String> {
    eval(format!(
        "dioxus.send(probe.action({},{}));",
        json!(kind),
        json!(value)
    ))
    .await
}
async fn frames() -> Result<(), String> {
    eval("await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));dioxus.send({ok:true});".into()).await.map(|_|())
}
fn native(state: &ShellState) -> Value {
    json!({"query":state.search_query,"revision":state.search_input_revision,"panel":state.search_filters.is_some(),
      "flags":state.search_filters.as_ref().map(|f|[f.has_file,f.has_link]),"error":state.search_filter_error,
      "results":state.search_results.len(),"page":state.search_pager.page,"loading":state.search_pager.loading,
      "noSession":state.core.session.is_none(),"noTransport":state.core.transport.is_none()})
}
async fn settled(
    state: Signal<ShellState>,
    query: &str,
    flags: Option<[bool; 2]>,
    error: Option<&str>,
) -> Result<(), String> {
    for _ in 0..100 {
        let ready = {
            let s = state.peek();
            isolated(&s)?;
            s.search_query == query
                && s.search_filters.as_ref().map(|f| [f.has_file, f.has_link]) == flags
                && s.search_filter_error == error
        };
        if ready {
            return frames().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Search did not settle: {}", native(&state.peek())))
}
async fn stage(
    state: Signal<ShellState>,
    name: &str,
    index: usize,
    records: &mut Vec<Value>,
) -> Result<(), String> {
    isolated(&state.peek())?;
    eval("probe.sample();dioxus.send({ok:true});".into()).await?;
    frames().await?;
    let dom = eval("dioxus.send(probe.sample());".into()).await?;
    let actual = native(&state.peek());
    if dom["query"] != actual["query"]
        || dom["panel"] != actual["panel"]
        || dom["flags"] != actual["flags"]
        || dom["alert"] != actual["error"]
        || dom["results"] != 0
        || actual["results"] != 0
        || actual["page"] != 0
        || actual["loading"] != false
    {
        return Err(format!("Search DOM/native mismatch: {dom} / {actual}"));
    }
    let value = json!({"ok":true,"stage":name,"ordinal":index,"noSession":true,"noTransport":true,"dom":dom,"native":actual});
    records.push(value.clone());
    eprintln!("search-has-filters-stage: {value}");
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing private capture directory")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    let path = directory.join(format!("{index}.ack"));
    for _ in 0..600 {
        let path = path.clone();
        let read = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture acknowledgement worker failed")?;
        isolated(&state.peek())?;
        match read {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Invalid capture acknowledgement".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot read capture acknowledgement".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Capture acknowledgement timeout".into())
}
async fn run(state: Signal<ShellState>) -> Result<Value, String> {
    isolated(&state.peek())?;
    // Initial mount is the only setup wait; every later state is produced by controls.
    eval("await new Promise((resolve,reject)=>{let n=0;const tick=()=>{if(document.querySelector('#search-filter-panel'))resolve();else if(++n>100)reject(Error('Search mount timeout'));else setTimeout(tick,20);};tick();});dioxus.send({ok:true});".into()).await?;
    let mut records = Vec::new();
    let mut actions = Vec::new();
    settled(state, QUERY, Some([true, false]), None).await?;
    stage(state, "initial", 0, &mut records).await?;
    actions.push(action("flag", "Has files").await?);
    settled(state, QUERY, Some([false, false]), None).await?;
    actions.push(action("flag", "Has links").await?);
    settled(state, QUERY, Some([false, true]), None).await?;
    stage(state, "pending", 1, &mut records).await?;
    actions.push(action("click", "Apply filters").await?);
    settled(state, APPLIED, None, None).await?;
    stage(state, "applied", 2, &mut records).await?;
    actions.push(action("click", "Filters").await?);
    settled(state, APPLIED, Some([false, true]), None).await?;
    actions.push(action("click", "Clear filters").await?);
    settled(state, CLEARED, None, None).await?;
    stage(state, "cleared", 3, &mut records).await?;
    actions.push(action("type", KEYBOARD).await?);
    settled(state, KEYBOARD, None, None).await?;
    actions.push(action("click", "Filters").await?);
    settled(state, KEYBOARD, Some([false, false]), None).await?;
    actions.push(action("flag", "Has files").await?);
    settled(state, KEYBOARD, Some([true, false]), None).await?;
    actions.push(action("enter", "").await?);
    settled(state, &format!("{KEYBOARD} has:file"), None, None).await?;
    stage(state, "keyboard-applied", 4, &mut records).await?;
    actions.push(action("type", UNFINISHED).await?);
    settled(state, UNFINISHED, None, None).await?;
    actions.push(action("click", "Filters").await?);
    settled(state, UNFINISHED, Some([false, false]), None).await?;
    actions.push(action("flag", "Has files").await?);
    settled(state, UNFINISHED, Some([true, false]), None).await?;
    actions.push(action("flag", "Has links").await?);
    settled(state, UNFINISHED, Some([true, true]), None).await?;
    let refused_revision = state.peek().search_input_revision;
    actions.push(action("click", "Apply filters").await?);
    settled(state, UNFINISHED, Some([true, true]), Some(ERROR)).await?;
    if state.peek().search_input_revision != refused_revision {
        return Err("Refusal remounted query".into());
    }
    stage(state, "quote-refused", 5, &mut records).await?;
    actions.push(action("type", "report \"unfinished\"").await?);
    settled(state, "report \"unfinished\"", None, None).await?;
    actions.push(action("click", "Filters").await?);
    settled(state, "report \"unfinished\"", Some([false, false]), None).await?;
    actions.push(action("flag", "Has files").await?);
    settled(state, "report \"unfinished\"", Some([true, false]), None).await?;
    actions.push(action("flag", "Has links").await?);
    settled(state, "report \"unfinished\"", Some([true, true]), None).await?;
    actions.push(action("click", "Apply filters").await?);
    settled(state, "report \"unfinished\" has:file has:link", None, None).await?;
    actions.push(action("click", "Filters").await?);
    settled(
        state,
        "report \"unfinished\" has:file has:link",
        Some([true, true]),
        None,
    )
    .await?;
    stage(state, "recovered", 6, &mut records).await?;
    isolated(&state.peek())?;
    let viewport = records.last().unwrap()["dom"]["viewport"].clone();
    Ok(
        json!({"ok":true,"fixture":fixture(),"noSession":true,"noTransport":true,
      "credentialBoundary":"actual search credentials refused; no raw request ledger", "records":records,"actions":actions,
      "refusedRevision":refused_revision,"layout":{"ok":true,"viewport":viewport}}),
    )
}
