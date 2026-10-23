//! Six-stage, exact opt-in native paragraph proof; baseline checker is unchanged.
use super::*;
use crate::canvas::text::fixture::formatted_title as fixture;

const DESIRED_HTML: &str =
    "<div>A <strong>bold</strong> line 🦊 日本語 &amp; details. <em>Added</em></div>";
const REFUSED_HTML: &str =
    "<div>A <strong>bold</strong> line 🦊 日本語 &amp; details. <em>Added</em> retained</div>";

pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-text-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop, r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] .canvas-text-open:not(:disabled)')})"#).await?;
    if !fixture::enabled(&state.peek()) {
        return Err("Expected disconnected formatted-title fixture".into());
    }
    let original = fixture::proof(&state.peek());
    let mut stages = Vec::new();
    open(desktop).await?;
    let initial_native =
        checked(desktop, include_str!("formatted_title.js")).await?["native"].clone();
    set_draft(desktop, DESIRED_HTML).await?;
    stage(state, desktop, "editing", 0, &mut stages).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    stage(state, desktop, "cancelled", 1, &mut stages).await?;
    if fixture::proof(&state.peek()) != original {
        return Err("Cancel changed original canonical custody".into());
    }
    open(desktop).await?;
    let reopened = checked(desktop, include_str!("formatted_title.js")).await?;
    if reopened["native"]["blocks"] != initial_native["blocks"] {
        return Err("Cancel lost original paragraph seed".into());
    }
    set_draft(desktop, DESIRED_HTML).await?;
    let duplicate = checked(desktop, r#"(()=>{const b=document.getElementById('canvas-text-save');if(!b||b.disabled)return {ok:false};b.click();b.click();return {ok:true,clicks:2,id:b.id};})()"#).await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');return {ok:e?.contentEditable==='false'&&document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled};})()"#).await?;
    if fixture::proof(&state.peek())["attempts"] != 1 {
        return Err("Duplicate Save dispatched more than once".into());
    }
    stage(state, desktop, "saving", 2, &mut stages).await?;
    if !fixture::complete(&mut state.write()) {
        return Err("Authored ACK/readback was not confirmed".into());
    }
    wait(desktop,r#"(()=>{const p=document.querySelector('[data-canvas-text-section="paragraph"]');return {ok:!document.getElementById('canvas-text-editor')&&p?.textContent.includes('Added')&&!p.querySelector('button')?.disabled};})()"#).await?;
    stage(state, desktop, "confirmed", 3, &mut stages).await?;
    open(desktop).await?;
    set_draft(desktop, REFUSED_HTML).await?;
    let before_refusal = checked(desktop, include_str!("formatted_title.js")).await?;
    if !fixture::change_preflight_title(&mut state.write()) {
        return Err("Could not stage original fresh-title response".into());
    }
    // No view/read invalidation: this actual Save must reach prepare/revalidate.
    click(desktop, "#canvas-text-save").await?;
    wait(desktop, r#"(()=>{const e=document.getElementById('canvas-text-draft'),root=document.getElementById('canvas-text-editor');return {ok:e?.isContentEditable&&!document.getElementById('canvas-text-cancel')?.disabled&&root?.querySelector(':scope > [role=alert]')?.textContent==='This text or your access changed. Cancel and refresh.'};})()"#).await?;
    let after_refusal = checked(desktop, include_str!("formatted_title.js")).await?;
    for field in ["blocks", "generation", "visit"] {
        if before_refusal["native"][field] != after_refusal["native"][field] {
            return Err(format!(
                "Refused title preflight changed original native {field}"
            ));
        }
    }
    stage(state, desktop, "title-refused", 4, &mut stages).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    stage(state, desktop, "cancelled-title", 5, &mut stages).await?;
    let final_source = fixture::proof(&state.peek());
    if final_source["attempts"] != 1
        || final_source["preflights"].as_array().map(Vec::len) != Some(2)
        || final_source["operation"] != Value::Null
    {
        return Err("Refused preflight issued or retained an operation".into());
    }
    let layout = checked(desktop, include_str!("formatted_title.js")).await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"attempts":1,
        "cancelNoWrite":true,"duplicateBlocked":true,"independentReadback":true,"titleRefusalPreserved":true,
        "syntheticInput":true,"httpFactoryExecuted":false,"original":original,"initialNative":initial_native,
        "duplicate":duplicate,"beforeRefusal":before_refusal,"afterRefusal":after_refusal,
        "stages":stages,"fixture":final_source,"layout":layout}),
    )
}

async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Formatted-title capture lost disconnected owner".into());
    }
    let before = fixture::proof(&state.peek());
    let geometry = wait(desktop, include_str!("formatted_title.js")).await?;
    let editor_layout = if geometry["native"].is_object() {
        editor_layout(desktop).await?
    } else {
        Value::Null
    };
    if fixture::proof(&state.peek()) != before {
        return Err("Original source changed during native projection".into());
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"fixture":before,"geometry":geometry,"editorLayout":editor_layout});
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Missing formatted-title capture directory")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    eprintln!("canvas-text-stage: {receipt}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture ACK worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => break,
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK read failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Capture ACK timed out at {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let after = checked(desktop, include_str!("formatted_title.js")).await?;
    if geometry != after || fixture::proof(&state.peek()) != before {
        return Err(format!(
            "Original title/draft/layout changed across {name} capture"
        ));
    }
    stages.push(receipt);
    Ok(())
}
