//! End-to-end closed List reply composition through the mounted editor and real controllers.
use super::*;
use crate::slack_lists::comments::fixture::{
    self, Model,
    compose::{self, Mode},
};
use std::time::Duration;
mod ack_socket;
mod deletions;
mod edits;
mod edits_ack;
mod harness;
mod incoming;
mod newer_draft;
mod reactions;
mod refresh_reactions;
mod storage_recovery;
const STAGES: &[&str] = &[
    "draft",
    "back",
    "back-reopen",
    "closed",
    "close-reopen",
    "escape-reopen",
    "scrim-reopen",
    "sending",
    "sent",
    "not-sent",
    "rejected",
    "unconfirmed",
    "locked-reopen",
    "confirmed",
    "denied",
    "record-held",
    "record-replaced",
    "account-held",
    "account-replaced",
];
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !compose::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let case = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").ok();
        let checked = match case.as_deref() {
            None | Some("compose") => run(state, &desktop).await,
            Some("newer-draft") => newer_draft::run(state, &desktop).await,
            Some("storage-recovery") => storage_recovery::run(state, &desktop).await,
            Some("reactions") => reactions::run(state, &desktop).await,
            Some("reaction-refresh") => refresh_reactions::run(state, &desktop).await,
            Some("deletions") => deletions::run(state, &desktop).await,
            Some("incoming") => incoming::run(state, &desktop).await,
            Some("edits") => edits::run(state, &desktop).await,
            Some("edit-ack") => edits_ack::run(state, &desktop).await,
            Some(case @ ("ack-socket-add" | "ack-socket-remove" | "ack-first")) => {
                ack_socket::run(state, &desktop, case).await
            }
            Some(_) => Err("Unknown List comments compose case".into()),
        };
        let result = match checked {
            Ok(v) => v,
            Err(e) => match case {
                Some(case) => json!({"ok":false,"case":case,"error":e}),
                None => json!({"ok":false,"error":e}),
            },
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-comments-compose-check: {result}\n").as_bytes())
                .expect("Compose terminal receipt");
        })
        .await
        .expect("Compose terminal writer");
    });
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    if id == "slack-list-comment-send" {
        // Native input capture and durable saving precede Send eligibility.
        // The explicit duplicate-click probes deliberately bypass this wait.
        settled(d).await?;
    }
    checked(d, &format!("window.__listCompose.click({})", json!(id))).await?;
    Ok(())
}
async fn open(d: &dioxus::desktop::DesktopContext, record: &str) -> Result<(), String> {
    checked(d, &format!("window.__listCompose.open({})", json!(record))).await?;
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==={}}})",
            json!(record)
        ),
    )
    .await?;
    click(d, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    Ok(())
}
async fn write(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    let v = eval(
        d,
        "({mounted:!!document.getElementById('slack-list-comment-draft')})",
    )
    .await?;
    if v["mounted"] != true {
        wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comment-write').disabled})").await?;
        click(d, "slack-list-comment-write").await?;
    }
    wait_dom(d,"({ok:!!window.superPlatinumRichEditor?.snapshot(document.getElementById('slack-list-comment-draft'))&&document.getElementById('slack-list-comment-draft')?.isContentEditable})").await?;
    Ok(())
}
async fn type_text(d: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    checked(d, &format!("window.__listCompose.type({})", json!(text))).await?;
    Ok(())
}
async fn wait_model(
    state: Signal<ShellState>,
    model: &Model,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..200 {
        let v = compose::snapshot(&state.peek(), model);
        if expected(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Compose model did not settle: {}",
        compose::snapshot(&state.peek(), model)
    ))
}
fn counts(v: &Value, prepared: usize, posts: usize) -> bool {
    v["composeLedger"]["prepared"]
        .as_array()
        .is_some_and(|r| r.len() == prepared)
        && v["composeLedger"]["posts"]
            .as_array()
            .is_some_and(|r| r.len() == posts)
}
async fn settled(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:window.__listCompose.snapshot().editor?.editable===true&&document.getElementById('slack-list-comment-send')?.disabled===false})").await?;
    Ok(())
}
async fn stable_drafts(state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    wait_model(state, model, |v| {
        v["draftStore"]["drafts"].as_array().is_some_and(|rows| {
            !rows.is_empty()
                && rows.iter().all(|e| {
                    e["busy"] == false
                        && e["saving"] == false
                        && e["loading"] == false
                        && e["terminalPending"] == false
                })
        })
    })
    .await
}
async fn stage(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(rows.len()) != Some(&name) {
        return Err("Compose stage order".into());
    }
    let expression = if matches!(name, "sending" | "record-held" | "account-held") {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===false&&document.getElementById('slack-list-comment-send')?.disabled===true;return v;})()"
    } else if matches!(name, "unconfirmed" | "locked-reopen") {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&!!v.editor&&document.getElementById('slack-list-comment-send')?.disabled===true;return v;})()"
    } else if matches!(name, "not-sent" | "rejected" | "denied") {
        "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true&&v.notices.length>0;return v;})()"
    } else {
        "window.__listCompose.snapshot()"
    };
    if name == "sent" {
        let sent =
            compose::snapshot(&state.peek(), model)["composeLedger"]["messages"][0]["ts"].clone();
        wait_dom(d, &format!("(()=>{{const v=window.__listCompose.snapshot();return {{ok:v.rows.filter(r=>r.ts==={sent}).length===1}};}})()")).await?;
    }
    let geometry = wait_dom(d, expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    if before["noSession"] != true || before["noTransport"] != true || before["writes"] != 0 {
        return Err("Compose fixture acquired live state".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = compose::snapshot(&state.peek(), model);
    if after != before {
        return Err(format!("Compose model changed during {name} capture"));
    }
    rows.push(
        json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,"capture":capture}),
    );
    Ok(())
}
async fn reopen(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    file: &super_platinum_core::slack::models::File,
) -> Result<(), String> {
    crate::slack_lists::open(state, file.clone());
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    write(d).await
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    let initial = compose::snapshot(&state.peek(), &model);
    let file = serde_json::from_value(initial["initialDocument"].clone())
        .map_err(|_| "Fixture file malformed")?;
    let team = state
        .peek()
        .core
        .active_team
        .clone()
        .ok_or("Fixture team missing")?;
    checked(d, include_str!("comments.js")).await?;
    checked(d, include_str!("compose_comments.js")).await?;
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
    wait_dom(
        d,
        "({ok:window.__listCompose.snapshot().editor?.bold===true})",
    )
    .await?;
    let mut original=checked(d,"(()=>{const v=window.__listCompose.snapshot();window.__composeOriginal=v.editor.blocks;return {ok:true,blocks:v.editor.blocks};})()").await?;
    original["seed"] = seed;
    let mut rows = vec![];
    stage(state, &model, d, "draft", original.clone(), &mut rows).await?;
    click(d, "slack-list-comments-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&document.activeElement===document.getElementById('slack-list-record-comments')})").await?;
    stage(state, &model, d, "back", Value::Null, &mut rows).await?;
    click(d, "slack-list-record-comments").await?;
    write(d).await?;
    stage(state, &model, d, "back-reopen", original.clone(), &mut rows).await?;
    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    stage(state, &model, d, "closed", Value::Null, &mut rows).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Closed draft was not safely persisted for journal recovery".into());
    }
    reopen(state, d, &file).await?;
    let mut recovered = original.clone();
    recovered["runtimeForgotten"] = json!(true);
    stage(state, &model, d, "close-reopen", recovered, &mut rows).await?;
    checked(d, "window.__listCompose.escape()").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&document.activeElement===document.getElementById('slack-list-record-comments')})").await?;
    click(d, "slack-list-record-comments").await?;
    write(d).await?;
    stage(
        state,
        &model,
        d,
        "escape-reopen",
        original.clone(),
        &mut rows,
    )
    .await?;
    checked(d, "window.__listCompose.scrim()").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    reopen(state, d, &file).await?;
    stage(
        state,
        &model,
        d,
        "scrim-reopen",
        original.clone(),
        &mut rows,
    )
    .await?;
    compose::mode(&model, Mode::HeldSuccess)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| {
        counts(v, 1, 1) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    checked(
        d,
        "(()=>{document.getElementById('slack-list-comment-send')?.click();return {ok:true};})()",
    )
    .await?;
    stage(state, &model, d, "sending", original.clone(), &mut rows).await?;
    let release = compose::release_post(&model)?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-draft')||document.getElementById('slack-list-comment-draft').innerText.trim()===''})").await?;
    stable_drafts(state, &model).await?;
    stage(
        state,
        &model,
        d,
        "sent",
        json!({"release":release}),
        &mut rows,
    )
    .await?;
    // Read back the same accepted row while its immediate local projection is
    // still mounted. A later reopen would discard that tail and miss dedup bugs.
    let reads_before = compose::snapshot(&state.peek(), &model)["ledger"]["requests"]
        .as_array()
        .ok_or("Missing comment read ledger")?
        .len();
    compose::publish(&model)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, &model, |v| {
        v["comments"]["loading"] == false
            && v["ledger"]["requests"]
                .as_array()
                .is_some_and(|r| r.len() == reads_before + 4)
    })
    .await?;
    let accepted_geometry = wait_dom(d, "window.__listCompose.snapshot()").await?;
    let accepted_readback = json!({
        "model":compose::snapshot(&state.peek(), &model),
        "geometry":accepted_geometry,
    });
    compose::unpublish(&model)?;
    write(d).await?;
    type_text(
        d,
        "Recoverable reply \u{65e5}\u{1f98a}\nKeep this exact draft.",
    )
    .await?;
    for (mode, name, prepared, posts) in [
        (Mode::NotSent, "not-sent", 2, 1),
        (Mode::Rejected, "rejected", 3, 2),
    ] {
        compose::mode(&model, mode)?;
        click(d, "slack-list-comment-send").await?;
        wait_model(state, &model, |v| counts(v, prepared, posts)).await?;
        stable_drafts(state, &model).await?;
        settled(d).await?;
        stage(state, &model, d, name, Value::Null, &mut rows).await?;
    }
    compose::mode(&model, Mode::HeldAmbiguous)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| {
        counts(v, 4, 3) && v["composeLedger"]["heldPost"] == true
    })
    .await?;
    let release = compose::release_post(&model)?;
    wait_dom(d,"({ok:document.getElementById('slack-list-comment-send')?.disabled&&document.getElementById('slack-list-comments')?.textContent.toLowerCase().includes('unconfirmed')})").await?;
    stable_drafts(state, &model).await?;
    stage(
        state,
        &model,
        d,
        "unconfirmed",
        json!({"release":release}),
        &mut rows,
    )
    .await?;
    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Unconfirmed draft was not safely persisted for journal recovery".into());
    }
    crate::slack_lists::open(state, file.clone());
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let mounted = eval(
        d,
        "({mounted:!!document.getElementById('slack-list-comment-draft')})",
    )
    .await?;
    if mounted["mounted"] != true {
        click(d, "slack-list-comment-write").await?;
    }
    stable_drafts(state, &model).await?;
    wait_model(state, &model, |v| {
        v["draftStore"]["drafts"].as_array().is_some_and(|rows| {
            rows.len() == 1
                && rows[0]["attempt"]["record"]["client_msg_id"]
                    == v["composeLedger"]["posts"][2]["args"]["client_msg_id"]
        })
    })
    .await?;
    wait_dom(d,"({ok:!!window.superPlatinumRichEditor?.snapshot(document.getElementById('slack-list-comment-draft'))&&document.getElementById('slack-list-comment-send')?.disabled===true})").await?;
    checked(d,"(()=>{document.getElementById('slack-list-comment-send')?.click();document.getElementById('slack-list-comment-send')?.click();return {ok:true};})()").await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    stage(
        state,
        &model,
        d,
        "locked-reopen",
        json!({"runtimeForgotten":true}),
        &mut rows,
    )
    .await?;
    compose::publish(&model)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments-refresh')?.disabled&&!document.getElementById('slack-list-comments')?.textContent.toLowerCase().includes('unconfirmed')})").await?;
    stable_drafts(state, &model).await?;
    stage(state, &model, d, "confirmed", Value::Null, &mut rows).await?;
    write(d).await?;
    type_text(d, "Permission refusal retains this draft.").await?;
    compose::mode(&model, Mode::Denied)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| {
        v["composeLedger"]["policies"].as_array().is_some_and(|p| {
            p.last()
                .is_some_and(|p| p["channel"]["is_non_threadable"] == true)
        })
    })
    .await?;
    stable_drafts(state, &model).await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-comment-draft')?.isContentEditable===true&&!!document.querySelector('#slack-list-comment-compose [role=alert]')})").await?;
    stage(state, &model, d, "denied", Value::Null, &mut rows).await?;
    click(d, "slack-list-comments-back").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    click(d, "slack-list-record-back").await?;
    open(d, "R_LIST_001").await?;
    write(d).await?;
    type_text(d, "Original synthetic item owns this draft.").await?;
    compose::mode(&model, Mode::HeldPreflight)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| v["composeLedger"]["held"] == true).await?;
    stage(state, &model, d, "record-held", Value::Null, &mut rows).await?;
    fixture::replace_record(&mut state.write())?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let release = compose::release(&model, &team)?;
    stable_drafts(state, &model).await?;
    stage(
        state,
        &model,
        d,
        "record-replaced",
        json!({"release":release}),
        &mut rows,
    )
    .await?;
    click(d, "slack-list-record-comments").await?;
    write(d).await?;
    let retry=eval(d,"({needed:!!document.getElementById('slack-list-comment-retry')&&document.getElementById('slack-list-comment-send')?.disabled===true})").await?;
    if retry["needed"] == true {
        click(d, "slack-list-comment-retry").await?;
        settled(d).await?;
    }
    compose::mode(&model, Mode::HeldPreflight)?;
    click(d, "slack-list-comment-send").await?;
    wait_model(state, &model, |v| v["composeLedger"]["held"] == true).await?;
    stage(state, &model, d, "account-held", Value::Null, &mut rows).await?;
    let (media, epoch) = {
        let s = state.peek();
        (s.media.clone(), s.core.account_epoch)
    };
    let mut replacement = ShellState::fixture(media);
    replacement.slack_lists = Default::default();
    replacement.overlay = None;
    replacement.core.active_account = Some("list-comments-compose-successor".into());
    replacement.core.account_epoch = epoch + 1;
    crate::bootstrap::install_loaded(state, replacement);
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let release = compose::release(&model, &team)?;
    stable_drafts(state, &model).await?;
    stage(
        state,
        &model,
        d,
        "account-replaced",
        json!({"release":release}),
        &mut rows,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,
        "final":compose::snapshot(&state.peek(),&model),"initialRecords":fixture::initial_records(&model),"acceptedReadback":accepted_readback,"noLiveNetwork":true}),
    )
}
