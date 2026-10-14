//! Native disconnected title transaction and compact-layout checks.
use super::*;
use crate::slack_links_check::wait_dom as shared_wait_dom;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let value = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => {
                let shell = state.peek();
                let draft = shell.profile_title.draft.as_ref();
                let fixture = shell.profile_title.fixture.as_ref().unwrap();
                json!({"ok":false,"error":e,"draft":draft.map(|d|json!({"visit":d.visit,"value":d.value,"baseline":d.baseline,"loading":d.loading,"current":d.token.current(&shell)})),"attempts":fixture.attempts,"reads":fixture.reads})
            }
        };
        eprintln!("profile-title-check: {value}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    ready(d).await?;
    input(d, "Cancelled").await?;
    click(d, "profile-title-cancel").await?;
    closed(d).await?;
    if !state
        .peek()
        .profile_title
        .fixture
        .as_ref()
        .unwrap()
        .attempts
        .is_empty()
    {
        return Err("Cancel wrote a title".into());
    }
    open(state);
    ready(d).await?;
    input(d, &"x".repeat(250)).await?;
    wait_dom(d,r#"({ok:document.getElementById('profile-title-save').disabled&&!!document.querySelector('[role=alert]')})"#).await?;
    input(d, "Jose\u{301} — designer").await?;
    wait_dom(
        d,
        r#"({ok:!document.getElementById('profile-title-save').disabled})"#,
    )
    .await?;
    wait_dom(d,r#"(()=>{const e=document.getElementById('profile-title-input');e.focus();e.setSelectionRange(2,2);window.__titleInput=e;const b=document.getElementById('profile-title-save');b.click();b.click();return{ok:true};})()"#).await?;
    wait_dom(d,r#"({ok:document.getElementById('profile-title-cancel').disabled&&document.getElementById('profile-title-save').disabled&&window.__titleInput===document.getElementById('profile-title-input')&&window.__titleInput.disabled&&window.__titleInput.selectionStart===2})"#).await?;
    {
        let s = state.peek();
        let f = s.profile_title.fixture.as_ref().unwrap();
        if f.attempts.len() != 1 || !f.valid {
            return Err("Duplicate or malformed Header write".into());
        }
    }
    fixture::complete(&mut state.write());
    closed(d).await?;
    {
        let s = state.peek();
        let o = Owner::capture(&s).unwrap();
        let p = s.core.workspaces[&o.team].users[&o.user]
            .profile
            .as_ref()
            .unwrap();
        if p.title.as_deref() != Some("José — designer")
            || p.status_text.as_deref() != Some("Reading")
            || p.pronouns.as_deref() != Some("they/them")
            || p.extra.get("title_fixture") != Some(&json!({"keep":true}))
        {
            return Err("Canonical title adoption altered siblings".into());
        }
    }
    state
        .write()
        .profile_title
        .fixture
        .as_mut()
        .unwrap()
        .uncertain = true;
    open(state);
    ready(d).await?;
    input(d, "").await?;
    click(d, "profile-title-save").await?;
    wait_dom(
        d,
        r#"({ok:document.getElementById('profile-title-cancel').disabled})"#,
    )
    .await?;
    fixture::complete(&mut state.write());
    wait_dom(d,r#"({ok:!!document.getElementById('profile-title-uncertain')&&!document.getElementById('profile-title-check').disabled})"#).await?;
    click(d, "profile-title-cancel").await?;
    closed(d).await?;
    open(state);
    wait_dom(d,r#"({ok:!!document.getElementById('profile-title-check')&&!document.getElementById('profile-title-check').disabled})"#).await?;
    click(d, "profile-title-check").await?;
    closed(d).await?;
    {
        let s = state.peek();
        let f = s.profile_title.fixture.as_ref().unwrap();
        if f.attempts.len() != 2
            || f.reads != 2
            || !f.valid
            || Owner::capture(&s).unwrap().title(&s) != ""
        {
            return Err("Clear or uncertain readback replayed/failed".into());
        }
    }
    state.write().profile_title.fixture.as_mut().unwrap().raw["result"]["data"]["user"]["profileSections"]
        [0]["profileElements"][0]["permissions"]["ui"] = json!(false);
    open(state);
    wait_dom(d,r#"({ok:document.getElementById('profile-title-input')?.disabled&&document.getElementById('profile-title-save').disabled&&document.getElementById('profile-title-reason')?.textContent.length>0})"#).await?;
    let layout=wait_dom(d,r#"(()=>{const e=document.querySelector('#shell-overlay-dialog.profile-title'),r=e?.getBoundingClientRect();const fields=[...e.querySelectorAll('input,footer button')];return{ok:!!r&&e.querySelectorAll('input').length===1&&e.querySelectorAll('footer button').length>=2&&r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth&&e.scrollWidth<=e.clientWidth+1&&fields.every(x=>{const b=x.getBoundingClientRect();return b.top>=r.top&&b.bottom<=r.bottom&&Math.abs(b.height-32)<2}),width:innerWidth,height:innerHeight};})()"#).await?;
    Ok(
        json!({"ok":true,"cancelNoWrite":true,"lengthValidation":true,"normalizedTitle":true,"duplicateGuard":true,"canonicalReadback":true,"siblingsPreserved":true,"clearAndUncertainNoReplay":true,"permissionReason":true,"caretPreserved":true,"layout":layout}),
    )
}
async fn ready(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,r#"({ok:!!document.querySelector('#shell-overlay-dialog.profile-title')?.dataset.keyboardMounted&&!!document.getElementById('profile-title-input')&&!document.getElementById('profile-title-input').disabled&&document.activeElement===document.getElementById('profile-title-input')})"#).await?;
    Ok(())
}
async fn closed(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(
        d,
        r#"({ok:!document.getElementById('profile-title-form')})"#,
    )
    .await?;
    Ok(())
}
async fn input(d: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    let v = serde_json::to_string(value).unwrap();
    wait_dom(d,&format!(r#"(()=>{{const e=document.getElementById('profile-title-input');if(!e||e.disabled)return{{ok:false}};e.value={v};e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true}};}})()"#)).await?;
    Ok(())
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let id = serde_json::to_string(id).unwrap();
    wait_dom(d,&format!(r#"(()=>{{const e=document.getElementById({id});if(!e||e.disabled)return{{ok:false}};e.click();return{{ok:true}};}})()"#)).await?;
    Ok(())
}

// Synthetic-only diagnostics preserve the exact failing predicate and native
// field state; neither real profile content nor session data enters this path.
async fn wait_dom(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    match shared_wait_dom(d, script).await {
        Ok(v) => Ok(v),
        Err(error) => {
            let diagnostic=shared_wait_dom(d,r#"(()=>{const e=document.getElementById('profile-title-input'),old=window.__titleInput,dialog=document.querySelector('#shell-overlay-dialog.profile-title');return{ok:true,keyboardMounted:dialog?.dataset.keyboardMounted,dialogGeneration:dialog?.dataset.dialogGeneration,activeId:document.activeElement?.id,activeTag:document.activeElement?.tagName,value:e?.value,selection:[e?.selectionStart,e?.selectionEnd],sameNode:e===old,oldConnected:old?.isConnected,oldValue:old?.value,oldSelection:[old?.selectionStart,old?.selectionEnd],inputDisabled:e?.disabled,saveDisabled:document.getElementById('profile-title-save')?.disabled,cancelDisabled:document.getElementById('profile-title-cancel')?.disabled,checkDisabled:document.getElementById('profile-title-check')?.disabled,message:document.getElementById('profile-title-message')?.textContent};})()"#).await;
            Err(format!(
                "Title predicate {script}: {error}; DOM={}",
                diagnostic.unwrap_or_else(|e| json!({"diagnosticError":e}))
            ))
        }
    }
}
