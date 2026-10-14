//! Actual review controls + isolated read injection + fresh-process persistence proof.
use super::{remote_review_fixture as fixture, *};
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Review check timed out"}),
        };
        eprintln!("draft-review-check: {receipt}");
    });
}
fn label(t: ComposerTarget) -> &'static str {
    if matches!(t, ComposerTarget::Thread) {
        "thread"
    } else {
        "channel"
    }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut e = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), e.recv::<Value>()).await;
    let _ = e.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Native receipt: {v}")),
        _ => Err("Native receipt timed out".into()),
    }
}
async fn review(target: ComposerTarget, operation: &str, extra: Value) -> Result<Value, String> {
    let mut args = extra;
    args["target"] = json!(label(target));
    eval(format!("{};try{{dioxus.send(await draftRemoteReviewCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("remote_review_check.js"),json!(operation),args)).await
}
async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(target)),"text":text,"input":input}))).await
}
async fn wait_panel(target: ComposerTarget) -> Result<Value, String> {
    for _ in 0..200 {
        if let Ok(v) = review(target, "read", json!({})).await {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Review did not open".into())
}
async fn wait_feedback(target: ComposerTarget) -> Result<Value, String> {
    for _ in 0..250 {
        if let Ok(v) = review(target, "feedback", json!({})).await {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Review did not finish".into())
}
async fn show(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<Value, String> {
    // Real component unmount/remount restores stored text. Never sets DOM text.
    {
        let mut s = state.write();
        s.main_view = MainView::Unreads;
        s.thread_root = None;
    }
    let mut removed = false;
    for _ in 0..100 {
        if native(target, "absent", None, false).await.is_ok() {
            removed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !removed {
        return Err("Composer did not unmount".into());
    }
    {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        s.sync_composer_drafts();
        if s.thread_root.is_some() {
            s.refresh_thread_from_core();
        }
    }
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    for _ in 0..150 {
        if let Ok(v) = native(target, "read", None, false).await
            && v["scope"] == scope
            && v["visit"].as_str().is_some_and(|s| !s.is_empty())
        {
            use super_platinum_core::domain::PendingScrollTarget;
            {
                let mut s = state.write();
                let id = s.core.active_channel.clone().ok_or("Channel missing")?;
                s.core.pending_scroll_to = Some((id, PendingScrollTarget::Latest));
                s.stick_to_bottom = true;
                s.reset_timeline_window();
            }
            if !crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await {
                return Err("Timeline anchor unavailable".into());
            }
            return native(target, "show", None, false).await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Restored composer mount unavailable".into())
}
async fn capture(
    d: &dioxus::desktop::DesktopContext,
    b: &fixture::Binding,
    name: &str,
) -> Result<Value, String> {
    eprintln!(
        "draft-review-stage: {}",
        json!({"stage":name,"edge":"before"})
    );
    let dir = std::env::var_os("SUPER_PLATINUM_DRAFT_REVIEW_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await?;
    let path = std::path::PathBuf::from(dir)
        .join(format!("{fixture}-{}-{}-{name}.png", b.scenario, b.phase));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    eprintln!(
        "draft-review-stage: {}",
        json!({"stage":name,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}
async fn flush(editor: &Arc<checkpoint::Editor>) -> Result<(), String> {
    // Parked conflict flush intentionally reports Stale; exact idle/error and
    // durable dictionaries below establish the preservation outcome instead.
    let _ = editor.flush().await;
    for _ in 0..250 {
        let s = editor.snapshot();
        if !s.running {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Editor did not settle".into())
}
async fn inventory_captures(
    target: ComposerTarget,
    b: &fixture::Binding,
    d: &dioxus::desktop::DesktopContext,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let mut review = wait_panel(target).await?;
    if review["selected"] != 0 || review["applyDisabled"] != true || review["noOverflow"] != true {
        return Err("Review must start without a default choice".into());
    }
    let rows = review["rows"]
        .as_array()
        .ok_or("Missing review inventory")?;
    if rows.len() != 6 {
        return Err(format!(
            "Expected full six-version inventory, got {}",
            rows.len()
        ));
    }
    for text in [
        fixture::BEFORE,
        fixture::BLOCKED,
        fixture::ALTERNATE,
        fixture::OLDER,
        fixture::AFTER,
    ] {
        if !rows
            .iter()
            .any(|r| r["text"].as_str().is_some_and(|s| s.contains(text)))
        {
            return Err(format!("Missing visible inventory text: {text}"));
        }
    }
    let count = rows.len();
    let mut readable = Vec::new();
    for index in 0..count {
        readable.push(review_row(target, index).await?);
        let name = format!("review-{index}");
        captures.insert(name.clone(), capture(d, b, &name).await?);
    }
    review["readableRows"] = json!(readable);
    Ok(review)
}
async fn review_row(target: ComposerTarget, index: usize) -> Result<Value, String> {
    review(target, "row", json!({"index":index})).await
}
async fn choose(target: ComposerTarget, kind: &str, text: Option<&str>) -> Result<(), String> {
    let v = review(target, "choose", json!({"kind":kind,"text":text})).await?;
    if v["selected"] != 1 || v["applyDisabled"] != false {
        return Err("Explicit selection did not enable Apply".into());
    }
    Ok(())
}
fn messages(shell: &ShellState) -> Value {
    json!(
        shell
            .core
            .workspaces
            .iter()
            .map(|(t, w)| (
                t,
                w.messages
                    .iter()
                    .map(|(id, bag)| (id, &bag.messages))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    )
}

async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let original = messages(&state.peek());
    let b = fixture::install(state).await?;
    fixture::wait_runner(&b).await?;
    let target = b.target();
    let key = fixture::key(target);
    let editor = b.editors[&key].clone();
    let initial = fixture::durable(&b).await?;
    let mut captures = serde_json::Map::new();
    let mounted = show(state, target).await?;
    let mut result = json!({"initialNative":mounted});
    if b.phase == "restart" {
        if mounted["text"] != initial[label(target)]["canonical"]["composer"]["text"] {
            return Err("Restart DOM did not render stored canonical text".into());
        }
        captures.insert("restart".into(), capture(d, &b, "restart").await?);
    } else {
        if mounted["text"] != fixture::BEFORE {
            return Err(format!("Initial native text mismatch: {mounted}"));
        }
        // Capture complete inventory before arming read races so all cases prove
        // no default selection and every compared version is inspectable.
        review(target, "open", json!({})).await?;
        result["initialReview"] = inventory_captures(target, &b, d, &mut captures).await?;
        if b.scenario == "typing-during-read-channel" {
            review(target, "cancel", json!({})).await?;
            b.arm("during-read")?;
            review(target, "open", json!({})).await?;
            b.wait_reached().await?;
            native(target, "set", Some(fixture::NEWER), true).await?;
            flush(&editor).await?;
            b.release();
            result["staleReadFeedback"] = wait_feedback(target).await?;
            if !result["staleReadFeedback"]["text"]
                .as_str()
                .is_some_and(|s| s.contains("saved versions changed"))
            {
                return Err("Read race did not report stale captured inventory".into());
            }
            if editor.snapshot().projection.is_some() {
                return Err("Stale read installed a choice".into());
            }
            result["preservedBeforeChoice"] = fixture::durable(&b).await?;
            captures.insert("handoff".into(), capture(d, &b, "handoff").await?);
            review(target, "open", json!({})).await?;
            wait_panel(target).await?;
            choose(target, "alternate", Some(fixture::NEWER)).await?;
        } else {
            if b.scenario == "typing-during-commit-thread" {
                b.arm("during-commit")?;
            }
            if matches!(
                b.scenario.as_str(),
                "changed-thread" | "deferred-composition-channel"
            ) {
                b.arm("after-commit")?;
            }
            choose(
                target,
                if b.scenario == "alternate-thread" {
                    "alternate"
                } else {
                    "remote"
                },
                (b.scenario == "alternate-thread").then_some(fixture::ALTERNATE),
            )
            .await?;
        }
        review(target, "apply", json!({})).await?;
        let second = matches!(
            b.scenario.as_str(),
            "typing-during-commit-thread" | "changed-thread" | "deferred-composition-channel"
        );
        if second {
            b.wait_reached().await?;
            match b.scenario.as_str() {
                "typing-during-commit-thread" => {
                    native(target, "set", Some(fixture::NEWER), true).await?;
                }
                "changed-thread" => {
                    native(target, "set", Some(fixture::NEWER), false).await?;
                }
                _ => {
                    native(target, "composition-start", None, false).await?;
                }
            }
            b.release();
            result["firstFeedback"] = wait_feedback(target).await?;
            let expected_feedback = match b.scenario.as_str() {
                "changed-thread" => "newer input was saved separately",
                "deferred-composition-channel" => "Finish composing",
                _ => "composer handoff is still pending",
            };
            if !result["firstFeedback"]["text"]
                .as_str()
                .is_some_and(|s| s.contains(expected_feedback))
            {
                return Err(format!(
                    "Unexpected first handoff classification: {}",
                    result["firstFeedback"]
                ));
            }
            flush(&editor).await?;
            if editor.snapshot().projection.is_none() {
                return Err("Deferred handoff lost its barrier".into());
            }
            result["preservedBeforeChoice"] = fixture::durable(&b).await?;
            captures.insert("handoff".into(), capture(d, &b, "handoff").await?);
            if b.scenario == "deferred-composition-channel" {
                native(target, "composition-end", None, false).await?;
                let before_native = native(target, "read", None, false).await?;
                let remounted = show(state, target).await?;
                if before_native["text"] != fixture::BEFORE
                    || remounted["text"] != fixture::BEFORE
                    || before_native["visit"] == remounted["visit"]
                {
                    return Err(
                        "Deferred composition remount changed predecessor or reused mount".into(),
                    );
                }
                let request_count = b.requests.lock().unwrap().len();
                review(target, "finish", json!({})).await?;
                result["finishFeedback"] = wait_feedback(target).await?;
                if b.requests.lock().unwrap().len() != request_count {
                    return Err("Finish repeated the explicit read".into());
                }
                result["finishAfterRemount"] =
                    json!({"before":before_native,"after":remounted,"noAdditionalRead":true});
            } else {
                // Real synthetic input makes native-ahead text the foreground
                // predecessor for the second explicit review; no state patch.
                native(target, "set", Some(fixture::NEWER), true).await?;
                flush(&editor).await?;
                review(target, "open", json!({})).await?;
                let second_review = wait_panel(target).await?;
                if second_review["selected"] != 0 || second_review["applyDisabled"] != true {
                    return Err("Second review selected a default".into());
                }
                result["secondReview"] = second_review;
                choose(target, "alternate", Some(fixture::NEWER)).await?;
                captures.insert(
                    "second-choice".into(),
                    capture(d, &b, "second-choice").await?,
                );
                review(target, "apply", json!({})).await?;
            }
        }
        result["feedback"] = wait_feedback(target).await?;
        flush(&editor).await?;
        if editor.snapshot().projection.is_some() {
            return Err(format!(
                "Final choice did not receive native acknowledgment: {}",
                result["feedback"]
            ));
        }
        if !captures.contains_key("handoff") {
            captures.insert("handoff".into(), capture(d, &b, "handoff").await?);
        }
        result["native"] = native(target, "read", None, false).await?;
        captures.insert("result".into(), capture(d, &b, "result").await?);
    }
    let durable = fixture::durable(&b).await?;
    let other = if matches!(target, ComposerTarget::Thread) {
        "channel"
    } else {
        "thread"
    };
    if durable[other] != initial[other] {
        return Err("Unrelated durable destination changed".into());
    }
    if b.phase == "exercise" {
        let expected = match b.scenario.as_str() {
            "alternate-thread" => fixture::ALTERNATE,
            "typing-during-read-channel" | "typing-during-commit-thread" | "changed-thread" => {
                fixture::NEWER
            }
            _ => fixture::AFTER,
        };
        let row = &durable[label(target)];
        if row["canonical"]["composer"]["text"] != expected
            || !row["blocked"].is_null()
            || row["alternates"] != json!([])
            || result["native"]["text"] != expected
        {
            return Err(format!(
                "Final explicit choice or inventory retirement mismatch: {}",
                json!({"durable":row,"native":result["native"]})
            ));
        }
        if b.scenario == "deferred-composition-channel"
            && durable != result["preservedBeforeChoice"]
        {
            return Err(
                "Finish mutated durable choice instead of only acknowledging native handoff".into(),
            );
        }
    } else if durable != initial {
        return Err("Restart mutated durable inventory".into());
    }
    let shell = state.peek();
    if !b.current(&shell) || messages(&shell) != original {
        return Err("Fixture ownership or canonical messages changed".into());
    }
    let requests = b
        .requests
        .lock()
        .map_err(|_| "Request ledger poisoned")?
        .clone();
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"scenario":b.scenario,"phase":b.phase,"nonce":b.nonce,"pid":std::process::id(),
        "noSession":true,"noTransport":true,"canonicalMessagesUnchanged":true,"syntheticInput":true,"syntheticIME":true,"noProductionRoute":true,
        "isolatedDatabase":b.path,"target":label(target),"preparedRequests":requests.len(),"attempts":requests,"initial":initial,"durable":durable,
        "routedInputs":b.routed.load(Ordering::Relaxed),"result":result,"captures":captures,"viewport":mounted["viewport"]}),
    )
}
