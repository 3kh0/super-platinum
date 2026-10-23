//! Real native mixed completion, late file response, private insertion and Save.
use super::*;
use super::{completion as source, completion_check as base, mentions, mentions_check as helper};
use dioxus::desktop::DesktopContext;
use serde_json::json;
use source::files;
use std::time::Duration;
mod empty_canvas;
mod link_title;
mod title_users;
const JS: &str = include_str!("file_completion_check.js");
const SHARED: &str = include_str!("member_completion_check.js");
const FAILURE: &str = "People suggestions could not load. Edit your query to try again.";

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !files::named() {
        return;
    }
    if super::link_completion::named() {
        link_title::start(state, desktop);
        return;
    }
    if super::empty_canvas::completion() {
        empty_canvas::start(state, desktop);
        return;
    }
    if source::title_users::named() {
        title_users::start(state, desktop);
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(190),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"error":error,"model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"File completion timed out","source":source::snapshot(&state.peek())})
            }
        };
        eprintln!("profile-file-completion-check: {result}");
    });
}
fn initial() -> Vec<Value> {
    let mut blocks = base::initial();
    for i in 1..=2 {
        blocks[0]["elements"][0]["elements"][i]["opaque"] = json!({"keep":[i,"original"]});
    }
    blocks
}
fn choices(shell: &ShellState) -> Result<Value, String> {
    super::super::rich::fixture_choices(shell).ok_or("Missing original mixed choices".into())
}
async fn choose(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    id: &str,
    keyboard: bool,
) -> Result<Value, String> {
    helper::wait_dom(
        desktop,
        &format!(
            "({JS})({},({SHARED}))",
            json!({"id":id,"keyboard":keyboard,"choices":choices(&state.peek())?})
        ),
    )
    .await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    index: Option<usize>,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if let Some(index) = index {
        helper::wait_dom(
            desktop,
            &format!(
                "({JS})({},({SHARED}))",
                json!({"action":"scroll","index":index})
            ),
        )
        .await?;
    }
    let raw = base::JS;
    let geometry = helper::wait_dom(
        desktop,
        &format!(
            "({JS})({{action:'layout',index:{},geometry:({raw})('layout',null)}},({SHARED}))",
            json!(index)
        ),
    )
    .await
    .map_err(|e| format!("File layout {stage}: {e}"))?;
    base::capture_measured(
        state, desktop, name, stage, proof, stages, captures, geometry,
    )
    .await
}
async fn mixed_ready(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<Value, String> {
    base::rows(
        desktop,
        &[
            files::CANVAS_LABEL,
            files::LIST_LABEL,
            "@Alex Local",
            "@Alex Harbor",
            "@Alex Retired",
            "@Alex Self",
        ],
        false,
    )
    .await?;
    let expected = choices(&state.peek())?;
    let rows = expected["values"].as_array().ok_or("Missing choices")?;
    if rows.len() != 7 || rows.iter().any(|r| r["value"]["file_id"] == files::HOSTED) {
        return Err(format!("Wrong mixed native-category choices: {expected}"));
    }
    for (kind, id) in [("canvas", files::CANVAS), ("file", files::LIST)] {
        if !rows
            .iter()
            .any(|r| r["value"]["type"] == kind && r["value"]["file_id"] == id)
        {
            return Err("Missing Canvas/list category".into());
        }
    }
    Ok(expected)
}
fn inserted(native: &Value, ids: &[&str]) -> Result<(), String> {
    let tokens = native["native"]["tokens"]
        .as_array()
        .ok_or("Missing native tokens")?;
    if tokens.len() != 2 + ids.len() {
        return Err("Wrong token count".into());
    }
    for (token, id) in tokens.iter().skip(2).zip(ids) {
        let kind = if *id == files::CANVAS {
            "canvas"
        } else {
            "file"
        };
        let href = format!("https://fixture.invalid/files/{id}");
        let expected = json!({"type":kind,"file_id":id,"url":href,"text":href});
        if token["value"] != expected {
            return Err("File wire identity/URL changed".into());
        }
    }
    Ok(())
}
fn people_counts(value: &Value) -> Value {
    json!([value["memberSearches"], value["groupSearches"]])
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !files::enabled(&state.peek()) || !mentions::user_mode() {
        return Err("Closed file fixture unavailable".into());
    }
    mentions::seed(state, "Alex Local").await?;
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, desktop).await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 2).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    capture(
        state,
        desktop,
        name,
        "opened",
        None,
        json!({"initialBlocks":initial()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if files::off() {
        base::query(desktop, "@alex").await?;
        base::rows(
            desktop,
            &["@Alex Local", "@Alex Harbor", "@Alex Retired", "@Alex Self"],
            false,
        )
        .await?;
        base::ledger(state, 0, 0).await?;
        let expected = choices(&state.peek())?;
        if expected["values"]
            .as_array()
            .is_none_or(|v| v.len() != 5 || v.iter().any(|r| r["value"]["type"] != "user"))
            || source::snapshot(&state.peek())["fileSearches"] != 0
        {
            return Err("Disabled gate admitted/dispatched files".into());
        }
        for (stage, index) in [
            ("disabled", 0),
            ("disabled-middle", 2),
            ("disabled-bottom", 4),
        ] {
            capture(
                state,
                desktop,
                name,
                stage,
                Some(index),
                json!({"choices":expected,"noFileRequest":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        return Ok(
            json!({"ok":true,"fixture":name,"disabled":true,"readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())}),
        );
    }
    files::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@alex").await?;
    base::rows(
        desktop,
        &["@Alex Local", "@Alex Harbor", "@Alex Retired", "@Alex Self"],
        false,
    )
    .await?;
    base::ledger(state, 1, 0).await?;
    let local = choices(&state.peek())?;
    if local["values"]
        .as_array()
        .is_none_or(|v| v.len() != 5 || v.iter().any(|r| r["value"]["type"] != "user"))
    {
        return Err("Held files blocked current people".into());
    }
    let before = source::snapshot(&state.peek());
    capture(
        state,
        desktop,
        name,
        "local-held",
        Some(0),
        json!({"choices":local,"beforeFiles":before}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != true {
        return Err("File reply undelivered".into());
    }
    base::ledger(state, 0, 0).await?;
    let mixed = mixed_ready(state, desktop).await?;
    if mixed["sequence"] != local["sequence"]
        || people_counts(&source::snapshot(&state.peek())) != people_counts(&before)
    {
        return Err("Late files refetched people/replaced query".into());
    }
    let source_files =
        super::super::rich::search::file_source::fixture_completion_proof(&state.peek())
            .map_err(|error| format!("Current file source proof: {error}"))?;
    let expected_channels = [
        (files::CANVAS, json!(["CZMAP001", "CAMAP001", "CTAIL001"])),
        (files::LIST, json!(["CYMAP002", "CBMAP002"])),
    ];
    for (id, channels) in expected_channels {
        if source_files["rows"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == id))
            .is_none_or(|row| row["effective"]["channels"] != channels)
        {
            return Err(format!("Original channel_map order lost for {id}"));
        }
    }
    let proof = json!({"choices":mixed,"release":release,"beforeFiles":before,"sameQuery":true,
        "fileResponseBytes":files::RESPONSE,"sourceFiles":source_files});
    for (stage, index) in [
        ("mixed", 0),
        ("mixed-middle", 2),
        ("mixed-lower", 4),
        ("mixed-bottom", 6),
    ] {
        capture(
            state,
            desktop,
            name,
            stage,
            Some(index),
            proof.clone(),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    let choice = choose(state, desktop, files::CANVAS, true).await?;
    inserted(&base::tokens(desktop, 3).await?, &[files::CANVAS])?;
    capture(
        state,
        desktop,
        name,
        "keyboard",
        None,
        json!({"file":files::CANVAS,"keyboard":true,"choice":choice}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::query(desktop, "@list").await?;
    base::rows(desktop, &[files::LIST_LABEL], false).await?;
    base::ledger(state, 0, 0).await?;
    let choice = choose(state, desktop, files::LIST, false).await?;
    let native = base::tokens(desktop, 4).await?;
    inserted(&native, &[files::CANVAS, files::LIST])?;
    let expected = native["native"]["snapshot"]["blocks"].clone();
    capture(
        state,
        desktop,
        name,
        "mouse",
        None,
        json!({"file":files::LIST,"keyboard":false,"choice":choice,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Use description", false).await?;
    helper::wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})").await?;
    if mentions::snapshot(&state.peek())["blocks"] != expected {
        return Err("Use changed file blocks".into());
    }
    helper::save(state, desktop, Outcome::Saved, 1, 1).await?;
    helper::click(desktop, "Edit description", false).await?;
    let native = base::tokens(desktop, 4).await?;
    inserted(&native, &[files::CANVAS, files::LIST])?;
    if native["native"]["snapshot"]["blocks"] != expected {
        return Err("File readback changed wire".into());
    }
    capture(
        state,
        desktop,
        name,
        "saved",
        None,
        json!({"exactReadback":true,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    // Actual keuO in-flight markers expire after 400 ms; never reset the cache
    // merely to force an independent negative request through its admission.
    tokio::time::sleep(Duration::from_millis(410)).await;
    files::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@pending-file").await?;
    base::ledger(state, 1, 0).await?;
    base::query(desktop, "@alex").await?;
    mixed_ready(state, desktop).await?;
    base::ledger(state, 0, 1).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Retired file response delivered".into());
    }
    capture(
        state,
        desktop,
        name,
        "query-retired",
        None,
        json!({"release":release,"tokensUnchanged":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    for (stage, term, mode) in [
        ("malformed", "@broken-file", source::Mode::Malformed),
        ("failure", "@offline-file", source::Mode::Failure),
    ] {
        tokio::time::sleep(Duration::from_millis(410)).await;
        files::mode(&state.peek(), mode).map_err(|e| e.to_string())?;
        base::query(desktop, term).await?;
        base::status(desktop, FAILURE).await?;
        base::ledger(state, 0, 1).await?;
        capture(
            state,
            desktop,
            name,
            stage,
            None,
            json!({"retainedBlocks":expected,"noWrite":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    tokio::time::sleep(Duration::from_millis(410)).await;
    files::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@cancel-file").await?;
    base::ledger(state, 1, 1).await?;
    helper::click(desktop, "Cancel description edit", false).await?;
    helper::wait_dom(
        desktop,
        "({ok:!document.getElementById('profile-section-rich')})",
    )
    .await?;
    base::ledger(state, 0, 2).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != false
        || release["cancelledBeforeReply"] != true
        || mentions::snapshot(&state.peek())["blocks"] != expected
    {
        return Err("Cancel changed draft/delivered reply".into());
    }
    capture(
        state,
        desktop,
        name,
        "cancelled",
        None,
        json!({"release":release,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"disabled":false,"readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"savedBlocks":expected,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek()),"limitations":["Synthetic request replies only; no HTTP.","DOM keyboard/mouse events, not OS IME.","Popup screenshot samples cover all seven mixed rows.","Group category retains separate accepted coverage; this fixture supplies positive empty group results."]}),
    )
}
