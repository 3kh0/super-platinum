//! Original Canvas editor metadata, read-only profile actions and rich reply custody.
use super::*;
use fixture::attribution as data;
mod capture;
mod recovery;

pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if data::recovery::requested() {
        recovery::start(state, desktop);
        return;
    }
    if !data::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_ROOT_ATTRIBUTION_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error,"proof":data::proof(&state.peek())}),
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("canvas-root-attribution-check: {result}")
        })
        .await;
    });
}
async fn ready(desktop: &dioxus::desktop::DesktopContext, kind: &str) -> Result<(), String> {
    wait(desktop,&format!("document.querySelector('[data-canvas-attribution]')?.dataset.canvasAttribution==={}&&!document.querySelector('#canvas-comments [aria-busy=true]')",json!(kind))).await
}
async fn refresh(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    editor: Option<&str>,
    visible: bool,
    real: bool,
    hidden: bool,
    kind: &str,
) -> Result<(), String> {
    let before = data::proof(&state.peek());
    let reads = before["requests"]
        .as_array()
        .ok_or("Missing original read ledger")?
        .len();
    let serial = before["retained"]["serial"]
        .as_u64()
        .ok_or("Missing original read serial")?;
    data::configure(&state.peek(), editor, visible, real, hidden)
        .map_err(|_| "Configure original fixture source failed")?;
    click(desktop, "#canvas-comments-refresh").await?;
    let mut accepted = false;
    for _ in 0..100 {
        let proof = data::proof(&state.peek());
        if proof["requests"].as_array().map(Vec::len) == Some(reads + 1)
            && proof["retained"]["serial"]
                .as_u64()
                .is_some_and(|v| v > serial)
            && proof["retained"]["loading"] == false
            && proof["retained"]["rows"][0]["document_comment"] == data::root()["document_comment"]
        {
            accepted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !accepted {
        return Err("Original refresh did not publish its next read/serial".into());
    }
    ready(desktop, kind).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    ready(desktop, "hidden").await?;
    let mut stages = Vec::new();
    capture::stage(state, desktop, "stream", &mut stages).await?;
    click(desktop, ".canvas-comment-open").await?;
    ready(desktop, "known").await?;
    capture::saved(state, desktop).await?;
    capture::stage(state, desktop, "thread", &mut stages).await?;
    eval(desktop, include_str!("attribution/input.js")).await?;
    capture::saved(state, desktop).await?;
    eval(desktop, include_str!("attribution/remember.js")).await?;
    capture::stage(state, desktop, "draft", &mut stages).await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('.canvas-attribution-name');if(!e)return {ok:false};e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();e.dispatchEvent(new MouseEvent('mouseenter',{bubbles:false,clientX:r.x+r.width/2,clientY:r.y+r.height/2}));return {ok:true};})()"#).await?;
    wait(
        desktop,
        "!!document.querySelector('.profile-hover .profile-name-button')",
    )
    .await?;
    capture::stage(state, desktop, "hover", &mut stages).await?;
    click(desktop, ".profile-hover .profile-name-button").await?;
    wait(
        desktop,
        "!!document.querySelector('#canvas-comment-profile .profile-close')",
    )
    .await?;
    data::hydrate(&mut state.write(), false).map_err(|_| "Synthetic profile hydration failed")?;
    wait(desktop,"!!document.querySelector('#canvas-comment-profile .profile-close')&&document.querySelector('#canvas-comment-profile .profile-title')?.textContent==='Synthetic loaded profile'").await?;
    capture::stage(state, desktop, "profile", &mut stages).await?;
    data::hydrate(&mut state.write(), true)
        .map_err(|_| "Original fixture profile restore failed")?;
    click(desktop, "#canvas-comment-profile .profile-close").await?;
    wait(
        desktop,
        "!document.getElementById('canvas-comment-profile')",
    )
    .await?;
    capture::stage(state, desktop, "returned", &mut stages).await?;
    for (name, editor, visible, real, hidden, kind) in [
        ("real-name", Some(data::EDITOR), true, true, false, "known"),
        (
            "hidden-name",
            Some(data::EDITOR),
            true,
            false,
            true,
            "known",
        ),
        ("empty-name", Some(data::EMPTY), true, false, false, "known"),
        (
            "unavailable",
            Some(data::UNKNOWN),
            true,
            false,
            false,
            "unavailable",
        ),
        (
            "hidden-root",
            Some(data::EDITOR),
            false,
            false,
            false,
            "hidden",
        ),
        ("missing-editor", None, true, false, false, "hidden"),
        (
            "editor-changed",
            Some(data::OTHER),
            true,
            false,
            false,
            "known",
        ),
    ] {
        refresh(state, desktop, editor, visible, real, hidden, kind).await?;
        capture::saved(state, desktop).await?;
        capture::stage(state, desktop, name, &mut stages).await?;
    }
    data::hold(&state.peek()).map_err(|_| "Cannot hold original policy read")?;
    refresh(
        state,
        desktop,
        Some(data::EDITOR),
        true,
        false,
        false,
        "pending",
    )
    .await?;
    capture::saved(state, desktop).await?;
    capture::stage(state, desktop, "policy-held", &mut stages).await?;
    refresh(
        state,
        desktop,
        Some(data::OTHER),
        true,
        false,
        false,
        "known",
    )
    .await?;
    for _ in 0..100 {
        if data::held_closed(&state.peek()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !data::held_closed(&state.peek()) {
        return Err("Original policy future did not retire".into());
    }
    let release = data::release(&state.peek()).map_err(|_| "Cannot release retired policy read")?;
    if release["delivered"] != false {
        return Err("Retired policy response was not cancelled".into());
    }
    capture::saved(state, desktop).await?;
    capture::stage(state, desktop, "stale-refused", &mut stages).await?;
    let proof = data::proof(&state.peek());
    if proof["requests"].as_array().map(Vec::len) != Some(11) {
        return Err("Comment read count differs".into());
    }
    Ok(
        json!({"ok":true,"noSession":false,"noTransport":false,"isolation":data::isolation(&state.peek()),
        "readOnly":true,"sourceIdentity":true,"draftPreserved":true,"profileVisible":true,"staleRefused":true,
        "requestCount":11,"proof":proof,"layout":stages.last().unwrap()["layout"],"stages":stages}),
    )
}
