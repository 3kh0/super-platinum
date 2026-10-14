//! Two-process runner supplies the same isolated SQLite path in seed/restore.
use super::super::delivery_check::fixture as read_fixture;
use super::*;
use serde_json::{Value, json};
use std::time::Duration;
const BUTTON: &str = "[data-upload-delivery='1999999999.000001'] .upload-delivery-check";
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_UPLOAD_RESTART_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("upload-restart" | "upload-restart-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("upload-restart-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Connected restart fixture refused".into());
    }
    for _ in 0..100 {
        if state.peek().core.pending_file_messages.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    {
        let shell = state.peek();
        if shell.core.pending_file_messages.len() != 2
            || shell.core.pending_file_messages.iter().any(|p| {
                p.attachments.iter().any(|a| {
                    !a.path.as_os_str().is_empty() || a.uploading || a.upload_cancel.is_some()
                })
            })
            || !shell.core.composer_attachments.is_empty()
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
        {
            return Err("Restart did not restore exactly two inert offline receipts".into());
        }
    }
    let initial = geometry(desktop, BUTTON).await?;
    let phase =
        std::env::var("SUPER_PLATINUM_UPLOAD_RESTART_PHASE").unwrap_or_else(|_| "restore".into());
    if phase == "seed" {
        let journal = state.peek().upload_journal.clone();
        if fixture::rows(journal).await?.len() != 2 {
            return Err("Seed did not commit two records".into());
        }
        return Ok(
            json!({"ok":true,"phase":"seed","persisted":2,"noTransport":true,"viewport":initial["viewport"]}),
        );
    }
    wait_dom(desktop,&format!("(()=>{{const b=document.querySelector({});if(!b||b.disabled)return {{ok:false}};b.focus();b.click();return {{ok:true}}}})()",json!(BUTTON))).await?;
    wait_dom(
        desktop,
        &format!(
            "({{ok:document.querySelector({})?.disabled===true}})",
            json!(BUTTON)
        ),
    )
    .await?;
    if read_fixture::calls(&state.peek()) != 1 {
        return Err("Read-only check count changed".into());
    }
    let page = read_fixture::canonical(&state.peek(), fixture::FIRST);
    read_fixture::complete(state, Ok(page));
    wait_dom(desktop,&format!("({{ok:!document.querySelector({})&&document.body.textContent.includes('Confirmed in the conversation.')}})",json!(BUTTON))).await?;
    let mut retired = false;
    for _ in 0..60 {
        let journal = state.peek().upload_journal.clone();
        let rows = fixture::rows(journal).await?;
        if rows.len() == 1 && rows[0].draft.client_msg_id == fixture::SECOND {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !retired {
        return Err("Canonical proof did not exactly retire the durable receipt".into());
    }
    {
        let shell = state.peek();
        if shell.core.pending_file_messages.len() != 1
            || shell.core.pending_file_messages[0].client_msg_id != fixture::SECOND
            || read_fixture::calls(&shell) != 1
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
        {
            return Err("Readback affected wrong receipt or connected transport".into());
        }
    }
    state.write().toast = None;
    let final_geometry = geometry(
        desktop,
        "[data-upload-delivery='1999999999.000002'] .upload-delivery-check",
    )
    .await?;
    Ok(
        json!({"ok":true,"phase":"restore","restored":2,"remaining":1,"requests":1,"noTransport":true,"noResend":true,"inertPaths":true,"exactRetirement":true,"initialVisible":initial["visible"],"visible":final_geometry["visible"],"viewport":final_geometry["viewport"]}),
    )
}
async fn geometry(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=document.querySelector({});if(!b)return {{ok:false}};b.scrollIntoView({{block:'nearest'}});b.focus();const row=b.closest('.upload-delivery');const visible=[b,row.querySelector('.upload-delivery-hint')].every(e=>{{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1}});return {{ok:visible&&row.scrollWidth<=row.clientWidth+1&&document.activeElement===b,visible,viewport:[innerWidth,innerHeight]}}}})()"#,json!(selector))).await
}
async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("Restart DOM did not settle: {last}"))
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |v| {
            let _ = tx.send(v);
        })
        .map_err(|_| "Evaluation failed")?;
    let raw = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| "Evaluation timed out")?
        .ok_or("Callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid evaluation")?;
    match value {
        Value::String(s) => {
            serde_json::from_str(&s).map_err(|_| "Invalid encoded evaluation".into())
        }
        v => Ok(v),
    }
}
