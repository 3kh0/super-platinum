//! Actual mounted RichEditor input/click fixture; synthetic events, never trusted IME.
use super::{
    check::{checked, click, editor_layout, set_draft, wait},
    checklist, underline_fixture as fixture,
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::{canvas::edit::text::TextIntent, slack::api::CanvasCollabSnapshot};
mod capture;
mod input;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_CANVAS_UNDERLINE_CHECK").as_deref() != Ok("1")
        || !fixture::requested()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-underline-check: {result}");
    });
}
fn snapshot(state: &Signal<ShellState>) -> Result<Arc<CanvasCollabSnapshot>, String> {
    checklist::presentation(&state.peek())
        .and_then(|v| v.snapshot)
        .ok_or("Missing original heading inventory".into())
}
fn count(state: &Signal<ShellState>, expected: usize) -> Result<(), String> {
    if !fixture::enabled(&state.peek()) || fixture::attempts(&state.peek()) != expected {
        Err("Underline dispatch count/isolation changed".into())
    } else {
        Ok(())
    }
}
async fn open(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    click(
        desktop,
        &format!("[data-canvas-text-section={}] .canvas-text-open", json!(id)),
    )
    .await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');let b;try{b=window.superPlatinumRichEditor.read(e.id)}catch{}return {ok:!!b&&e.isContentEditable&&document.activeElement===e&&['Heading','Paragraph'].includes(document.getElementById('canvas-text-draft-label')?.textContent)};})()"#).await?;
    editor_layout(desktop).await?;
    Ok(())
}
async fn save(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-text-save").await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');return {ok:e?.contentEditable==='false'&&document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled};})()"#).await?;
    // Actual disabled duplicate activation, no controller shortcut.
    checked(desktop,r#"(()=>{document.getElementById('canvas-text-save').click();document.getElementById('canvas-text-cancel').click();return {ok:true};})()"#).await?;
    Ok(())
}
async fn close(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    Ok(())
}
fn canonical(
    before: &CanvasCollabSnapshot,
    after: &CanvasCollabSnapshot,
    id: &str,
    level: u8,
    rtml: &str,
) -> Result<(), String> {
    if before.sections.len() != after.sections.len() {
        return Err("Underline edit changed inventory length".into());
    }
    for (old, new) in before.sections.iter().zip(&after.sections) {
        if old.id() != new.id() {
            return Err("Underline edit reordered identities".into());
        }
        if old.id() != id {
            if old != new {
                return Err(format!("Underline edit changed sibling {}", old.id()));
            }
        } else {
            let a = stable_fields(old.raw());
            let b = stable_fields(new.raw());
            if a != b || new.style().unwrap() != Some(u64::from(level)) {
                return Err("Underline edit changed structure/opaque bytes".into());
            }
            let text = new
                .raw()
                .message(12)
                .unwrap()
                .unwrap()
                .message(1)
                .unwrap()
                .unwrap();
            if text.string(1).unwrap() != Some(rtml) || text.string(2).unwrap() != Some("ja") {
                return Err("Underline text/language differs".into());
            }
            let intent = TextIntent::capture(after, id, checklist::save::now_usec().unwrap())
                .map_err(|_| "Saved heading could not reopen")?;
            if intent.heading_level() != (level > 0).then_some(level) {
                return Err("Saved heading level differs".into());
            }
        }
    }
    Ok(())
}
fn stable_fields(raw: &super_platinum_core::canvas::wire::RawMessage) -> Vec<u8> {
    let mut fields = raw
        .fields()
        .iter()
        .filter(|f| !matches!(f.tag(), 2 | 12 | 38))
        .collect::<Vec<_>>();
    // merge_partial appends present ACK tags. Normalize only re-emitted
    // identity fields; all other original field bytes AND order stay exact.
    fields.sort_by_key(|f| match f.tag() {
        1 | 6 | 7 => f.tag(),
        _ => 0,
    });
    fields
        .into_iter()
        .flat_map(|f| raw.encoded_field(f).unwrap())
        .copied()
        .collect()
}
async fn rendered(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    level: u8,
    text: &str,
) -> Result<Value, String> {
    wait(desktop,&format!(r#"(()=>{{const p=document.querySelector('[data-canvas-text-section="'+{}+'"]'),h=p?.querySelector('.canvas-paragraph-content');if(h)h.scrollIntoView({{block:'nearest'}});return {{ok:!document.getElementById('canvas-text-editor')&&h?.tagName==={}&&h.textContent==={}&&!h.querySelector('p,div')&&!p.querySelector('.canvas-text-open')?.disabled,tag:h?.tagName,text:h?.textContent}};}})()"#,json!(id),json!(if level == 0 { "DIV".into() } else { format!("H{level}") }),json!(text))).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] .canvas-text-open:not(:disabled)')})"#).await?;
    count(&state, 0)?;
    let original = snapshot(&state)?;
    let mut stages = Vec::new();
    rendered(desktop, "paragraph", 0, "Outside under & 中🙂\nTail toggle").await?;
    capture::stage(state, desktop, "initial", 0, &mut stages).await?;
    open(desktop, "heading-two").await?;
    set_draft(desktop, "<div><u>Cancelled 中🙂</u></div>").await?;
    capture::stage(state, desktop, "cancel-draft", 1, &mut stages).await?;
    close(desktop).await?;
    count(&state, 0)?;
    if snapshot(&state)?.sections != original.sections {
        return Err("Cancel changed original sections".into());
    }
    open(desktop, "heading-one").await?;
    set_draft(desktop, "<div><em><u>Retained stale 日🦊</u></em></div>").await?;
    let team = state
        .peek()
        .core
        .active_team
        .clone()
        .ok_or("Missing fixture team")?;
    if !checklist::invalidate(&mut state.write(), &team, "Ffixture") {
        return Err("Source did not invalidate".into());
    }
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');return {ok:document.getElementById('canvas-text-save')?.disabled&&!document.getElementById('canvas-text-cancel')?.disabled&&e.textContent==='Retained stale 日🦊'};})()"#).await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('canvas-text-save').click();return {ok:true};})()"#,
    )
    .await?;
    count(&state, 0)?;
    capture::stage(state, desktop, "stale-draft", 2, &mut stages).await?;
    close(desktop).await?;
    fixture::refresh(&mut state.write());
    open(desktop, "paragraph").await?;
    input::edit_and_toggle(desktop).await?;
    capture::stage(state, desktop, "paragraph-toolbar", 3, &mut stages).await?;
    let edits = [
        (
            "paragraph",
            0,
            "",
            "Before Outside <u>unINNER der &amp; 中🙂</u><br>Tail <u>toggle</u>",
            "Before Outside unINNER der & 中🙂\nTail toggle",
        ),
        (
            "heading-one",
            1,
            "<div><strong><u>First 中🙂</u></strong><br><br><em><u>Second 日🦊</u></em> &amp; &lt;literal&gt;</div>",
            "<b><u>First 中🙂</u></b><br><br><i><u>Second 日🦊</u></i> &amp; &lt;literal&gt;",
            "First 中🙂\n\nSecond 日🦊 & <literal>",
        ),
        (
            "heading-two",
            2,
            "<div>Second <em><u>heading 中🙂</u></em> &amp; details</div>",
            "Second <i><u>heading 中🙂</u></i> &amp; details",
            "Second heading 中🙂 & details",
        ),
        (
            "heading-three",
            3,
            "<div>Third <u><s>heading 中🙂</s></u> &lt;done&gt;</div>",
            "Third <u><del>heading 中🙂</del></u> &lt;done&gt;",
            "Third heading 中🙂 <done>",
        ),
    ];
    let mut previous = snapshot(&state)?;
    for (index, (id, level, html, rtml, text)) in edits.iter().enumerate() {
        if index > 0 {
            open(desktop, id).await?;
            set_draft(desktop, html).await?;
        }
        save(desktop).await?;
        count(&state, index + 1)?;
        if index == 0 {
            capture::stage(state, desktop, "paragraph-saving", 4, &mut stages).await?;
        }
        if !fixture::complete(&mut state.write()) {
            return Err("Underline ACK did not settle".into());
        }
        rendered(desktop, id, *level, text).await?;
        let after = snapshot(&state)?;
        canonical(&previous, &after, id, *level, rtml)?;
        previous = after;
        capture::stage(
            state,
            desktop,
            [
                "paragraph-confirmed",
                "h1-confirmed",
                "h2-confirmed",
                "h3-confirmed",
            ][index],
            5 + index,
            &mut stages,
        )
        .await?;
    }
    open(desktop, "heading-three").await?;
    set_draft(
        desktop,
        "<div><strong><u>Unconfirmed 中🙂</u></strong> keep this</div>",
    )
    .await?;
    save(desktop).await?;
    count(&state, 5)?;
    if !fixture::uncertain(&mut state.write()) {
        return Err("Request did not become uncertain".into());
    }
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.textContent.includes('Unconfirmed')})"#).await?;
    capture::stage(state, desktop, "uncertain", 9, &mut stages).await?;
    for (ordinal, name) in [(10, "readback-once"), (11, "readback-twice")] {
        let reads = fixture::proof(&state.peek())["reads"].as_u64().unwrap();
        wait(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Refresh');if(!b||b.disabled)return {ok:false};b.click();return {ok:true};})()"#).await?;
        for _ in 0..80 {
            if fixture::proof(&state.peek())["reads"].as_u64() == Some(reads + 1) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        if fixture::proof(&state.peek())["reads"].as_u64() != Some(reads + 1) {
            return Err("Explicit underline readback did not finish".into());
        }
        wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft'),s=document.getElementById('canvas-text-save'),c=document.getElementById('canvas-text-cancel');return {ok:e?.textContent==='Unconfirmed 中🙂 keep this'&&s.disabled&&c.disabled&&s.textContent.includes('Unconfirmed')};})()"#).await?;
        checked(desktop,r#"(()=>{document.getElementById('canvas-text-save').click();document.getElementById('canvas-text-cancel').click();return {ok:true};})()"#).await?;
        count(&state, 5)?;
        if snapshot(&state)?.sections != previous.sections {
            return Err("Unconfirmed readback rewrote canonical sections".into());
        }
        capture::stage(state, desktop, name, ordinal, &mut stages).await?;
    }
    let layout = editor_layout(desktop).await?;
    Ok(
        json!({"ok":true,"attempts":5,"confirmed":4,"cancelNoWrite":true,"staleDraftPreserved":true,"duplicateBlocked":true,"canonicalFormatting":true,"structurePreserved":true,"toolbarToggle":true,"insideOutsideInput":true,"uncertainNoReplay":true,"repeatedReadbackLocked":true,"syntheticInput":true,"noSession":true,"noTransport":true,"layout":layout,"original":fixture::inventory(&original),"stages":stages,"fixture":fixture::proof(&state.peek())}),
    )
}
