//! Disconnected end-to-end native projection. Synthetic input/IME, isolated SQLite.
use super::{projection_fixture as fixture, *};
use crate::{
    state::{ComposerTarget, MainView, ShellState},
    view::composer::projection::{self, ProjectionResult},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(90), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Projection check timed out"}),
        };
        eprintln!("draft-projection-check: {receipt}");
    });
}
fn field(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "thread-composer"
    } else {
        "channel-composer"
    }
}
fn label(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "thread"
    } else {
        "channel"
    }
}
async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    let args = json!({"id":field(target),"text":text,"input":input});
    let script = format!(
        "{}; try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,reason:String(error)}});}} await dioxus.recv();",
        include_str!("projection_check.js"),
        serde_json::to_string(operation).unwrap(),
        args
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let _ = eval.send(());
    match result {
        Ok(Ok(v)) => Ok(v),
        _ => Err("Native fixture receipt unavailable".into()),
    }
}
async fn require_ready<T>(
    result: ProjectionResult<T>,
    target: ComposerTarget,
    stage: &str,
) -> Result<T, String> {
    let classification = match result {
        ProjectionResult::Ready(value) => return Ok(value),
        ProjectionResult::Changed(_) => "Changed".to_owned(),
        ProjectionResult::Deferred(reason) => format!("Deferred({reason:?})"),
    };
    // This controller is exact-gated to synthetic fixtures; no production text
    // or account state is included in diagnostics.
    let snapshot = match native(target, "read", None, false).await {
        Ok(snapshot) => snapshot,
        Err(error) => json!({"error":error}),
    };
    Err(json!({"stage":stage,"target":label(target),"classification":classification,"native":snapshot}).to_string())
}
async fn wait_field(target: ComposerTarget, present: bool) -> Result<(), String> {
    for _ in 0..100 {
        let value = native(
            target,
            if present { "ready" } else { "absent" },
            None,
            false,
        )
        .await?;
        if value["ok"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("{} mount did not settle", label(target)))
}
fn mounted_current(value: &Value, scope: &str, retired: Option<&str>) -> bool {
    value["ok"] == true
        && value["scope"] == scope
        && value["visit"]
            .as_str()
            .is_some_and(|visit| !visit.is_empty() && retired != Some(visit))
}
async fn wait_current(
    state: Signal<ShellState>,
    target: ComposerTarget,
    retired: Option<&str>,
) -> Result<Value, String> {
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    let mut last = Value::Null;
    for _ in 0..100 {
        last = native(target, "read", None, false).await?;
        if crate::view::composer::field_scope(&state.peek(), target) != scope {
            return Err("Fixture destination changed while awaiting mount".into());
        }
        if mounted_current(&last, &scope, retired) {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(json!({"error":"Current destination mount did not settle","scope":scope,"retiredVisit":retired,"native":last}).to_string())
}
async fn restore_timeline(mut state: Signal<ShellState>) -> Result<(), String> {
    use super_platinum_core::domain::PendingScrollTarget;
    let before = crate::view::composer::field_scope(&state.peek(), ComposerTarget::Channel);
    let channel = state
        .peek()
        .core
        .active_channel
        .clone()
        .ok_or("Missing fixture channel")?;
    {
        let mut shell = state.write();
        shell.core.pending_scroll_to = Some((channel.clone(), PendingScrollTarget::Latest));
        shell.stick_to_bottom = true;
        shell.reset_timeline_window();
    }
    if !crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await {
        return Err("Fixture remounted timeline latest anchor did not land".into());
    }
    let mut shell = state.write();
    if crate::view::composer::field_scope(&shell, ComposerTarget::Channel) != before {
        return Err("Timeline restoration changed composer identity".into());
    }
    if shell.core.pending_scroll_to == Some((channel, PendingScrollTarget::Latest)) {
        shell.core.pending_scroll_to = None;
    }
    Ok(())
}
async fn show(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<(), String> {
    {
        let mut shell = state.write();
        shell.main_view = MainView::Home;
        shell.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        shell.sync_composer_drafts();
        if shell.thread_root.is_some() {
            shell.refresh_thread_from_core();
        }
    }
    wait_current(state, target, None).await?;
    native(target, "show", None, false).await?;
    Ok(())
}
async fn capture(d: &dioxus::desktop::DesktopContext, name: &str) -> Result<Value, String> {
    eprintln!(
        "draft-projection-stage: {}",
        json!({"stage":name,"edge":"before"})
    );
    let dir = std::env::var_os("SUPER_PLATINUM_DRAFT_PROJECTION_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let scenario = std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    let phase =
        std::env::var("SUPER_PLATINUM_DRAFT_PROJECTION_PHASE").map_err(|_| "Missing phase")?;
    // Two rendering frames separate state mutations from their evidence PNG.
    let mut eval = dioxus::document::eval(
        "requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send(true))); await dioxus.recv();",
    );
    tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>())
        .await
        .map_err(|_| "Paint timeout")?
        .map_err(|_| "Paint receipt")?;
    let _ = eval.send(());
    let path =
        std::path::PathBuf::from(dir).join(format!("{fixture}-{scenario}-{phase}-{name}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    eprintln!(
        "draft-projection-stage: {}",
        json!({"stage":name,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}
async fn durable(binding: &fixture::Binding) -> Result<Value, String> {
    // Fresh Account avoids inferring persistence from the writer's cached View.
    let reader = Account::new(binding.account.backend.clone());
    let mut rows = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let key = fixture::key(target);
        let review = reader
            .owner(key.clone())
            .review_local()
            .await
            .map_err(|e| format!("Readback: {e:?}"))?;
        let stored = review
            .stored
            .as_ref()
            .ok_or("Missing canonical observation")?;
        let snapshot = stored.state.desired().ok_or("Missing canonical snapshot")?;
        let decoded = codec::decode(&key, snapshot).map_err(|_| "Canonical snapshot decode")?;
        if review.local.composer.text != fixture::AFTER || decoded.composer().text != fixture::AFTER
        {
            return Err("Canonical observed draft changed or blocked".into());
        }
        let needs_alternate = matches!(
            binding.scenario.as_str(),
            "ahead"
                | "input-before"
                | "input-after"
                | "input-between"
                | "composition-input-first"
                | "composition-end-first"
        );
        if needs_alternate {
            if review.blocked.as_ref().is_none_or(|b| {
                b.draft.composer.text != fixture::NEWER
                    || b.draft.composer.reply_broadcast != matches!(target, ComposerTarget::Thread)
            }) {
                return Err("Exact blocked local version missing".into());
            }
            if review.alternates.len() != 1
                || review.alternates[0].draft.composer.text != fixture::NEWER
                || review.alternates[0].draft.composer.reply_broadcast
                    != matches!(target, ComposerTarget::Thread)
            {
                return Err("Exact newest local alternate missing".into());
            }
        } else if !review.alternates.is_empty() || review.blocked.is_some() {
            return Err("Unexpected local authorship during projection".into());
        }
        rows.insert(label(target).into(),json!({"key":key,"canonical":review.local,"blocked":review.blocked.as_ref().map(|b|json!({"token":b.token,"local":b.draft})),"storageRevision":stored.revision,"snapshot":snapshot,"alternates":review.alternates.iter().map(|a|json!({"writer":a.writer_id,"serial":a.revision,"local":a.draft})).collect::<Vec<_>>() }));
    }
    Ok(Value::Object(rows))
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let original_rows = canonical_rows(&state.peek());
    let binding = fixture::install(state).await?;
    fixture::wait_runner(&binding).await?;
    let mut captures = serde_json::Map::new();
    let mut results = serde_json::Map::new();
    if binding.phase == "restart" {
        let readback = durable(&binding).await?;
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            show(state, target).await?;
            // Restart mounts canonical storage content; no old barrier/DOM claim.
            remount(state, target).await?;
            let native = native(target, "read", None, false).await?;
            if native["text"] != fixture::AFTER {
                return Err("Restart mount did not render loaded canonical draft".into());
            }
            results.insert(label(target).into(), json!({"ok":true,"native":native}));
            captures.insert(label(target).into(), capture(d, label(target)).await?);
        }
        return finish(state, &binding, original_rows, readback, results, captures);
    }
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let prepared = fixture::observation(&state.peek(), target)?;
        binding.editors[&fixture::key(target)]
            .observe_event(prepared)
            .await
            .map_err(|e| format!("Observe: {e:?}"))?;
    }
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        show(state, target).await?;
        let initial = native(target, "selection", None, false).await?;
        if initial["text"] != fixture::BEFORE
            || initial["direction"] != "backward"
            || initial["display"] != initial["text"]
            || initial["start"].as_u64()
                != Some(
                    fixture::BEFORE[..fixture::BEFORE
                        .find(char::from_u32(0x1f680).unwrap())
                        .unwrap()]
                        .encode_utf16()
                        .count() as u64,
                )
            || initial["end"].as_u64() != Some(fixture::BEFORE.encode_utf16().count() as u64)
        {
            return Err("Canonical plain Unicode/backward selection setup failed".into());
        }
        captures.insert(
            format!("{}-before", label(target)),
            capture(d, &format!("{}-before", label(target))).await?,
        );
        let editor = binding.editors[&fixture::key(target)].clone();
        let barrier = editor
            .snapshot()
            .projection
            .ok_or("Missing projection barrier")?;
        let ticket = editor
            .prepare_projection(&barrier.id)
            .await
            .map_err(|e| format!("Projection ticket: {e:?}"))?;
        let serial = ticket.serial();
        if binding.scenario == "ahead" {
            native(target, "set", Some(fixture::NEWER), false).await?;
        }
        let captured = projection::capture_projection(state, target, editor.clone(), ticket).await;
        let mut preservation = Value::Null;
        let mut navigation = Value::Null;
        if binding.scenario == "ahead" {
            let ProjectionResult::Changed(changed) = captured else {
                return Err("DOM-ahead capture was not Changed".into());
            };
            let receipt = changed
                .preserve(&editor)
                .await
                .map_err(|e| format!("Preserve native input: {e:?}"))?;
            preservation = json!({"writer":receipt.writer_id(),"serial":receipt.serial(),"storageRevision":receipt.storage_revision(),"local":receipt.local()});
        } else {
            let attempt = require_ready(captured, target, "quiescent capture").await?;
            match binding.scenario.as_str() {
                "apply" => {
                    let applied =
                        require_ready(attempt.apply().await, target, "native apply").await?;
                    let unrelated = DraftKey {
                        team: "T1".into(),
                        channel: "C3".into(),
                        thread: None,
                    };
                    let unrelated_local = fixture::local(
                        &format!("Unrelated dirty draft survives {}", label(target)),
                        ComposerTarget::Channel,
                    );
                    state
                        .write()
                        .core
                        .drafts
                        .capture(&unrelated, &unrelated_local.composer, &[]);
                    let other = if matches!(target, ComposerTarget::Channel) {
                        fixture::key(ComposerTarget::Thread)
                    } else {
                        fixture::key(ComposerTarget::Channel)
                    };
                    let other_before = state.peek().core.drafts.items.get(&other).cloned();
                    let installed = applied
                        .install()
                        .map_err(|e| format!("Paired install: {e:?}"))?;
                    {
                        let mut shell = state.write();
                        let changes = shell.core.drafts.take_changes();
                        if changes.len() != 1
                            || changes
                                .get(&unrelated)
                                .is_none_or(|v| !same_local(v, &unrelated_local))
                            || shell
                                .core
                                .drafts
                                .items
                                .get(&other)
                                .map(|v| serde_json::to_value(v).unwrap())
                                != other_before
                                    .as_ref()
                                    .map(|v| serde_json::to_value(v).unwrap())
                        {
                            return Err("Clean projection lost unrelated draft state or marked target dirty".into());
                        }
                    }
                    editor
                        .acknowledge_projection(installed)
                        .await
                        .map_err(|e| format!("Acknowledge: {e:?}"))?;
                    if editor.snapshot().projection.is_some() {
                        return Err("Successful acknowledgment retained barrier".into());
                    }
                }
                "input-before" => {
                    let before = binding.routed.load(Ordering::Relaxed);
                    native(target, "queued-input", Some(fixture::NEWER), true).await?;
                    if editor.snapshot().serial <= serial
                        || binding.routed.load(Ordering::Relaxed) <= before
                    {
                        return Err("Native input did not synchronously submit before apply".into());
                    }
                    if matches!(attempt.apply().await, ProjectionResult::Ready(_)) {
                        return Err("Pre-apply native input was overwritten".into());
                    }
                }
                "input-between" => {
                    let applied =
                        require_ready(attempt.apply().await, target, "input-between apply").await?;
                    native(target, "set", Some(fixture::NEWER), true).await?;
                    if editor.snapshot().serial <= serial || applied.install().is_ok() {
                        return Err(
                            "Input between native apply and paired install was overwritten".into(),
                        );
                    }
                }
                "selection" => {
                    native(target, "selection-change", None, false).await?;
                    if !matches!(
                        attempt.apply().await,
                        ProjectionResult::Deferred(
                            projection::ProjectionDeferral::SelectionChanged
                        )
                    ) {
                        return Err("Caret change did not defer projection".into());
                    }
                }
                "input-after" => {
                    let applied =
                        require_ready(attempt.apply().await, target, "input-after apply").await?;
                    let installed = applied
                        .install()
                        .map_err(|e| format!("Paired install: {e:?}"))?;
                    native(target, "set", Some(fixture::NEWER), true).await?;
                    if editor.snapshot().serial <= serial
                        || editor.acknowledge_projection(installed).await.is_ok()
                    {
                        return Err("Input after provisional install did not invalidate ack".into());
                    }
                }
                "composition-input-first" | "composition-end-first" => {
                    native(target, "composition-start", None, false).await?;
                    native(target, "set", Some(fixture::NEWER), false).await?;
                    if matches!(attempt.apply().await, ProjectionResult::Ready(_)) {
                        return Err("Composition allowed native replacement".into());
                    }
                    if binding.scenario == "composition-input-first" {
                        native(target, "set", Some(fixture::NEWER), true).await?;
                        native(target, "composition-end", None, false).await?;
                    } else {
                        native(target, "composition-end", None, false).await?;
                        native(target, "set", Some(fixture::NEWER), true).await?;
                    }
                }
                "remount" => {
                    // Independent explicit proposals test both sides of native apply.
                    remount(state, target).await?;
                    if matches!(attempt.apply().await, ProjectionResult::Ready(_)) {
                        return Err("Retired native node authorized apply".into());
                    }
                    let ticket = editor
                        .prepare_projection(&barrier.id)
                        .await
                        .map_err(|e| format!("Fresh remount ticket: {e:?}"))?;
                    let fresh = require_ready(
                        projection::capture_projection(state, target, editor.clone(), ticket).await,
                        target,
                        "fresh remount capture",
                    )
                    .await?;
                    let applied =
                        require_ready(fresh.apply().await, target, "pre-remount apply").await?;
                    remount(state, target).await?;
                    if applied.install().is_ok() {
                        return Err("Retired app-owned mount authorized install".into());
                    }
                    if native(target, "read", None, false).await?["scope"] != initial["scope"] {
                        return Err("Remount test changed field scope".into());
                    }
                }
                "navigation" => {
                    navigation = navigate(state, target).await?;
                    if matches!(attempt.apply().await, ProjectionResult::Ready(_)) {
                        return Err("Old navigation visit authorized apply".into());
                    }
                }
                _ => return Err("Unknown fixture scenario".into()),
            }
        }
        if binding.scenario != "apply" {
            let _ = editor.flush().await;
            if editor.snapshot().projection.is_none() {
                return Err("Deferred/stale projection cleared barrier".into());
            }
        }
        let expected = if matches!(
            binding.scenario.as_str(),
            "ahead"
                | "input-before"
                | "input-after"
                | "input-between"
                | "composition-input-first"
                | "composition-end-first"
        ) {
            fixture::NEWER
        } else if binding.scenario == "apply" {
            fixture::AFTER
        } else {
            fixture::BEFORE
        };
        let native = native(target, "read", None, false).await?;
        if native["text"] != expected {
            let shell = state.peek();
            let key = fixture::key(target);
            let live = if matches!(target, ComposerTarget::Thread) {
                &shell.core.thread_composer
            } else {
                &shell.core.composer
            };
            return Err(json!({"error":"Native text changed unexpectedly","target":label(target),"expected":expected,"native":native,
                "expectedScope":crate::view::composer::field_scope(&shell,target),"live":live,"book":shell.core.drafts.items.get(&key),"navigation":navigation}).to_string());
        }
        captures.insert(
            format!("{}-result", label(target)),
            capture(d, &format!("{}-result", label(target))).await?,
        );
        results.insert(label(target).into(),json!({"ok":true,"native":native,"preservation":preservation,"navigation":navigation,"serial":editor.snapshot().serial,"barrierRetained":editor.snapshot().projection.is_some()}));
    }
    if binding.scenario == "navigation" {
        let target = ComposerTarget::Thread;
        let editor = binding.editors[&fixture::key(target)].clone();
        let barrier = editor
            .snapshot()
            .projection
            .ok_or("Navigation barrier lost")?;
        let ticket = editor
            .prepare_projection(&barrier.id)
            .await
            .map_err(|e| format!("Account switch ticket: {e:?}"))?;
        let ProjectionResult::Ready(attempt) =
            projection::capture_projection(state, target, editor.clone(), ticket).await
        else {
            return Err("Account switch capture unavailable".into());
        };
        {
            let mut shell = state.write();
            shell.core.active_account = Some("00000000-0000-4000-8000-000000000079".into());
            shell.core.account_epoch += 1;
        }
        if matches!(attempt.apply().await, ProjectionResult::Ready(_))
            || editor.snapshot().projection.is_none()
        {
            return Err("Account switch accepted old projection".into());
        }
    }
    let readback = durable(&binding).await?;
    finish(state, &binding, original_rows, readback, results, captures)
}
fn canonical_rows(shell: &ShellState) -> Value {
    json!(
        shell
            .core
            .workspaces
            .iter()
            .map(|(team, w)| (
                team,
                w.messages
                    .iter()
                    .map(|(id, bag)| (id, &bag.messages))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    )
}
fn finish(
    state: Signal<ShellState>,
    binding: &fixture::Binding,
    original_rows: Value,
    readback: Value,
    results: serde_json::Map<String, Value>,
    captures: serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let shell = state.peek();
    if shell.core.transport.is_some()
        || shell.core.session.is_some()
        || canonical_rows(&shell) != original_rows
    {
        return Err("Disconnected fixture changed canonical message rows".into());
    }
    Ok(
        json!({"ok":true,"phase":binding.phase,"scenario":binding.scenario,"nonce":binding.nonce,"pid":std::process::id(),"noTransport":true,"noSession":true,"canonicalMessagesUnchanged":true,"preparedRequests":0,"syntheticInput":true,"syntheticIME":true,"noProductionRoute":true,"isolatedDatabase":binding.path,"routedInputs":binding.routed.load(Ordering::Relaxed),"results":results,"durable":readback,"viewport":results["thread"]["native"]["viewport"],"captures":captures}),
    )
}
async fn remount(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<(), String> {
    let view = state.peek().main_view;
    let before = wait_current(state, target, None).await?;
    {
        let mut shell = state.write();
        if matches!(target, ComposerTarget::Channel) {
            shell.main_view = MainView::Unreads
        } else {
            shell.thread_root = None;
        }
    }
    wait_field(target, false).await?;
    {
        let mut shell = state.write();
        shell.main_view = view;
        if matches!(target, ComposerTarget::Thread) {
            shell.thread_root = Some(fixture::ROOT.into());
            shell.refresh_thread_from_core();
        }
    }
    let after = wait_current(state, target, before["visit"].as_str()).await?;
    if after["scope"] != before["scope"] {
        return Err("Same-scope remount changed scope".into());
    }
    if matches!(target, ComposerTarget::Channel) {
        restore_timeline(state).await?;
    }
    Ok(())
}
async fn navigate(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<Value, String> {
    let before = wait_current(state, target, None).await?;
    let key = fixture::key(target);
    let book = state
        .peek()
        .core
        .drafts
        .items
        .get(&key)
        .cloned()
        .ok_or("Navigation source draft missing")?;
    let mut intermediate = Value::Null;
    if matches!(target, ComposerTarget::Channel) {
        let original = state.peek().active_channel;
        {
            let mut shell = state.write();
            let other = shell
                .channels
                .iter()
                .position(|c| c.id == "C1")
                .ok_or("Missing alternate channel")?;
            shell.select_channel(other, crate::state::ChannelOpen::Global);
        }
        intermediate = wait_current(state, target, before["visit"].as_str()).await?;
        if intermediate["scope"] == before["scope"] {
            return Err("Intermediate channel did not change scope".into());
        }
        state
            .write()
            .select_channel(original, crate::state::ChannelOpen::Global);
    } else {
        {
            let mut shell = state.write();
            shell.close_thread();
        }
        wait_field(target, false).await?;
        {
            let mut shell = state.write();
            shell.thread_root = Some(fixture::ROOT.into());
            shell.thread_generation += 1;
            shell.sync_composer_drafts();
            shell.refresh_thread_from_core();
        }
    }
    let after = wait_current(
        state,
        target,
        if intermediate.is_null() {
            before["visit"].as_str()
        } else {
            intermediate["visit"].as_str()
        },
    )
    .await?;
    if after["scope"] == before["scope"] || after["visit"] == before["visit"] {
        return Err("Reopened destination reused old composer identity".into());
    }
    {
        let shell = state.peek();
        let live = if matches!(target, ComposerTarget::Thread) {
            &shell.core.thread_composer
        } else {
            &shell.core.composer
        };
        if live != &book.composer
            || shell
                .core
                .drafts
                .items
                .get(&key)
                .is_none_or(|current| !same_local(current, &book))
            || live.text != fixture::BEFORE
        {
            return Err(json!({"error":"Navigation changed live/book predecessor","native":after,"live":live,"book":shell.core.drafts.items.get(&key),"beforeBook":book}).to_string());
        }
    }
    restore_timeline(state).await?;
    Ok(json!({"before":before,"intermediate":intermediate,"after":after,"liveBookPreserved":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mount_readiness_requires_final_scope_and_fresh_visit_not_text() {
        let old = json!({"ok":true,"scope":"B","visit":"2","text":fixture::BEFORE});
        assert!(!mounted_current(&old, "A-new", Some("1")));
        let current = json!({"ok":true,"scope":"A-new","visit":"3","text":"arbitrary"});
        assert!(mounted_current(&current, "A-new", Some("2")));
        assert!(!mounted_current(&current, "A-new", Some("3")));
        assert!(!mounted_current(
            &json!({"ok":true,"scope":"A-new","visit":""}),
            "A-new",
            None
        ));
    }
}
