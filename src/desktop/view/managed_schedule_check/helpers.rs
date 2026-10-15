//! Native composer and storage helpers shared only by the opted-in Schedule check.
use super::*;
pub(super) fn label(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Channel => "channel",
        ComposerTarget::Thread => "thread",
    }
}
pub(super) fn ensure(value: bool, reason: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(reason.into()) }
}
pub(super) async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Managed Schedule DOM: {value}")),
        _ => Err("Managed Schedule DOM timeout".into()),
    }
}
pub(super) async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_| ())
}
pub(super) async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
        include_str!("../../drafts/projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(target)),"text":text,"input":input}))).await
}
pub(super) fn native_valid(value: &Value, text: &str) -> bool {
    value["text"] == text
        && value["display"] == text
        && value["codePoints"] == json!(text.chars().map(|c| c as u32).collect::<Vec<_>>())
        && value["utf16Length"] == json!(text.encode_utf16().count())
}
pub(super) fn same_native(before: &Value, after: &Value) -> bool {
    [
        "text",
        "display",
        "codePoints",
        "utf16Length",
        "scope",
        "visit",
        "start",
        "end",
        "direction",
    ]
    .iter()
    .all(|key| before[*key] == after[*key])
}
async fn anchor_measure(expected: &Value) -> Result<Value, String> {
    eval(format!(
        "{};dioxus.send({{ok:true,anchor:scheduleHomeAnchor({})}});await dioxus.recv();",
        include_str!("home.js"),
        expected
    ))
    .await
    .map(|v| v["anchor"].clone())
}
fn same_timeline(state: Signal<ShellState>, expected: &Value) -> bool {
    let shell = state.peek();
    json!(shell.timeline_scope()) == expected["scope"]
        && json!(shell.channel_rows.len()) == expected["windowEnd"]
        && json!(shell.channel_rows.last().map(|m| &m.id)) == expected["latestId"]
}
pub(super) async fn home(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Value, String> {
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
    ensure(removed, "Composer did not unmount")?;
    use super_platinum_core::domain::PendingScrollTarget;
    let expected = {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        s.sync_composer_drafts();
        if s.thread_root.is_some() {
            s.refresh_thread_from_core();
        }
        // Publish the intended virtual window together with the Home mount.
        // A mounted composer alone does not prove that its timeline tail exists.
        let id = s.core.active_channel.clone().ok_or("Channel missing")?;
        s.core.pending_scroll_to = Some((id, PendingScrollTarget::Latest));
        s.stick_to_bottom = true;
        s.reset_timeline_window();
        json!({"scope":s.timeline_scope(),"latestId":s.channel_rows.last().map(|m|&m.id),"windowEnd":s.channel_rows.len()})
    };
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    let mut last = Value::Null;
    for _ in 0..150 {
        ensure(
            same_timeline(state, &expected),
            "Home timeline identity changed while mounting",
        )?;
        let field = native(target, "read", None, false).await.ok();
        last = anchor_measure(&expected).await?;
        if let Some(field) = field
            && field["scope"] == scope
            && field["visit"].as_str().is_some_and(|s| !s.is_empty())
            && last["ready"] == true
        {
            let before = last;
            ensure(
                same_timeline(state, &expected),
                "Home timeline changed before navigation",
            )?;
            let landed =
                crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await;
            frames().await?;
            let mut value = native(target, "show", None, false).await?;
            let after = anchor_measure(&expected).await?;
            if !landed
                || !same_timeline(state, &expected)
                || after["atBottom"] != true
                || before["inputGeneration"] != after["inputGeneration"]
                || field["scope"] != value["scope"]
                || field["visit"] != value["visit"]
            {
                return Err(format!(
                    "Timeline anchor did not settle: {}",
                    json!({"landed":landed,"before":before,"after":after,"sameTimeline":same_timeline(state,&expected)})
                ));
            }
            value["homeAnchor"] = json!({"landed":landed,"before":before,"after":after});
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Restored composer/timeline mount unavailable: {last}"
    ))
}
async fn flush(editor: &Arc<checkpoint::Editor>) -> Result<(), String> {
    let _ = editor.flush().await;
    for _ in 0..250 {
        if !editor.snapshot().running {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Managed Schedule editor did not settle".into())
}
pub(super) async fn type_text(
    binding: &fixture::Binding,
    target: ComposerTarget,
    text: &str,
) -> Result<Value, String> {
    let routed = binding.routed.load(Ordering::Relaxed);
    native(target, "selection", None, false).await?;
    native(target, "set", Some(text), true).await?;
    for _ in 0..150 {
        if binding.routed.load(Ordering::Relaxed) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        binding.routed.load(Ordering::Relaxed) > routed,
        "Native input did not reach the isolated writer",
    )?;
    flush(&binding.editors[&fixture::key(target)]).await?;
    let durable = fixture::durable(binding).await?;
    ensure(
        durable[label(target)]["canonical"]["composer"]["text"] == text,
        "Native input did not persist exact text",
    )?;
    let value = native(target, "read", None, false).await?;
    ensure(
        native_valid(&value, text),
        "Native Unicode input differs from canonical text",
    )?;
    Ok(value)
}

pub(super) fn messages(shell: &ShellState) -> Value {
    let channel = shell.core.workspaces["T1"].messages.get("C2");
    let thread = shell
        .core
        .threads
        .get(&("T1".into(), "C2".into(), fixture::ROOT.into()));
    let delivery: serde_json::Map<String, Value> = shell
        .pending_delivery
        .iter()
        .map(|(ts, status)| {
            (
                ts.clone(),
                json!(match status {
                    PendingDeliveryStatus::Failed => "Failed",
                    PendingDeliveryStatus::Unconfirmed => "Unconfirmed",
                }),
            )
        })
        .collect();
    json!({"channel":channel.map(|bag|&bag.messages),"thread":thread.map(|bag|&bag.messages),
        "pending":{"channel":channel.map(|bag|bag.pending.clone()).unwrap_or_default(),"thread":thread.map(|bag|bag.pending.clone()).unwrap_or_default()},
        "pending_delivery":delivery})
}

pub(super) fn queue(shell: &ShellState) -> Value {
    let rows: std::collections::BTreeMap<_, _> = shell.core.scheduled.items.iter().map(|(id,row)| (id.clone(),json!({
        "id":row.id,"date_scheduled":row.post_at,"date_created":row.date_created,"client_msg_id":row.client_msg_id,
        "blocks":row.blocks,"file_ids":row.file_ids,"destinations":row.destinations,"is_deleted":row.is_deleted,"is_sent":row.is_sent,"last_updated_ts":row.last_updated_ts
    }))).collect();
    json!(rows)
}

pub(super) fn context(shell: &ShellState) -> Value {
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"viewId":shell.draft_writes.view_id,"team":shell.core.active_team})
}
