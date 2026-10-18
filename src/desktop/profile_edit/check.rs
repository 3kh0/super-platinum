//! Native synthetic own-profile form proof. No transport or real profile writes.
use super::*;
use crate::slack_links_check::wait_dom;
use serde_json::json;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if sections::fixture::interaction_enabled(&state.peek()) {
        sections::fixture::check::start(state, desktop);
        return;
    }
    if !fixture::enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("profile-edit-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait_dom(desktop,r#"({ok:document.getElementById('profile-edit-real_name')?.value==='Original Person'&&!document.getElementById('profile-edit-real_name').disabled})"#).await?;
    input(desktop, "display_name", "Cancelled").await?;
    click(desktop, "profile-edit-cancel").await?;
    closed(desktop).await?;
    if !state
        .peek()
        .profile_edit
        .fixture
        .as_ref()
        .unwrap()
        .attempts
        .is_empty()
    {
        return Err("Cancel dispatched a profile write".into());
    }
    open(state);
    ready(desktop).await?;
    input(desktop, "real_name", "").await?;
    wait_dom(desktop,r#"({ok:document.getElementById('profile-edit-save').disabled&&document.querySelector('[role=alert]')?.textContent.includes('full name')})"#).await?;
    input(desktop, "real_name", "Original Person").await?;
    input(desktop, "display_name", "Jose\u{301}").await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('profile-edit-save').disabled})"#,
    )
    .await?;
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('profile-edit-display_name');e.focus();e.setSelectionRange(2,2);window.__profileField=e;return{ok:true};})()"#).await?;
    input(desktop, "pronouns", "she/her").await?;
    wait_dom(desktop,r#"({ok:window.__profileField===document.getElementById('profile-edit-display_name')&&window.__profileField.selectionStart===2})"#).await?;
    wait_dom(desktop,r#"(()=>{const b=document.getElementById('profile-edit-save');b.click();b.click();return{ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:['profile-edit-save','profile-edit-cancel','profile-edit-real_name','profile-edit-display_name','profile-edit-pronouns'].every(id=>document.getElementById(id)?.disabled)})"#).await?;
    {
        let shell = state.peek();
        let fixture = shell.profile_edit.fixture.as_ref().unwrap();
        if fixture.attempts != vec![json!({"display_name":"José","pronouns":"she/her"})]
            || !fixture.valid
        {
            return Err("Duplicate or malformed profile write".into());
        }
        if fixture.profile.display_name.as_deref() != Some("Original") {
            return Err("Profile changed before canonical confirmation".into());
        }
    }
    fixture::complete(&mut state.write());
    closed(desktop).await?;
    {
        let shell = state.peek();
        let owner = Owner::capture(&shell).unwrap();
        let p = shell.core.workspaces[&owner.team].users[&owner.user]
            .profile
            .as_ref()
            .unwrap();
        if p.display_name.as_deref() != Some("José")
            || p.pronouns.as_deref() != Some("she/her")
            || p.status_text.as_deref() != Some("Reading")
            || p.extra.get("unknown_fixture") != Some(&json!({"keep":true}))
        {
            return Err("Canonical text/unknown/status merge differs".into());
        }
    }
    state
        .write()
        .profile_edit
        .fixture
        .as_mut()
        .unwrap()
        .uncertain = true;
    open(state);
    ready(desktop).await?;
    input(desktop, "pronouns", "").await?;
    click(desktop, "profile-edit-save").await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('profile-edit-cancel').disabled})"#,
    )
    .await?;
    fixture::complete(&mut state.write());
    wait_dom(desktop,r#"({ok:!!document.getElementById('profile-edit-uncertain')&&!document.getElementById('profile-edit-check').disabled})"#).await?;
    click(desktop, "profile-edit-cancel").await?;
    closed(desktop).await?;
    open(state);
    wait_dom(desktop,r#"({ok:!!document.getElementById('profile-edit-check')&&document.getElementById('profile-edit-pronouns').disabled})"#).await?;
    click(desktop, "profile-edit-check").await?;
    closed(desktop).await?;
    {
        let shell = state.peek();
        let fixture = shell.profile_edit.fixture.as_ref().unwrap();
        if fixture.attempts.len() != 2 || fixture.reads != 2 || !fixture.valid {
            return Err("Checking replayed a save or skipped canonical readback".into());
        }
    }
    // Known field restriction is rendered with a reason; other fields remain usable.
    state
        .write()
        .profile_edit
        .fixture
        .as_mut()
        .unwrap()
        .allow_display = false;
    open(state);
    ready(desktop).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('profile-edit-display_name').disabled&&!!document.getElementById('profile-edit-display_name-hint')&&!document.getElementById('profile-edit-real_name').disabled})"#).await?;
    let layout = wait_dom(desktop, LAYOUT).await?;
    Ok(
        json!({"ok":true,"cancelNoWrite":true,"requiredValidation":true,"normalizedChangedFields":true,"duplicateGuard":true,
        "canonicalReadback":true,"statusAndUnknownPreserved":true,"uncertainReopenNoReplay":true,"permissionReason":true,"caretPreserved":true,"layout":layout}),
    )
}
async fn ready(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.getElementById('profile-edit-real_name')&&!document.getElementById('profile-edit-real_name').disabled})"#).await?;
    Ok(())
}
async fn closed(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('profile-edit-form')})"#,
    )
    .await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let id = serde_json::to_string(id).unwrap();
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({id});if(!e||e.disabled)return{{ok:false}};e.click();return{{ok:true}};}})()"#)).await?;
    Ok(())
}
async fn input(
    desktop: &dioxus::desktop::DesktopContext,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let id = serde_json::to_string(&format!("profile-edit-{key}")).unwrap();
    let value = serde_json::to_string(value).unwrap();
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({id});if(!e||e.disabled)return{{ok:false}};e.value={value};e.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true}};}})()"#)).await?;
    Ok(())
}
const LAYOUT: &str = r#"(()=>{const d=document.querySelector('.profile-edit'),r=d?.getBoundingClientRect();const fields=[...d.querySelectorAll('input')],buttons=[...d.querySelectorAll('footer button')];const fits=e=>{const b=e.getBoundingClientRect();return b.width>0&&b.left>=r.left&&b.right<=r.right+1&&b.top>=r.top&&b.bottom<=r.bottom+1&&Math.abs(b.height-32)<2;};return{ok:!!r&&r.top>=0&&r.bottom<=innerHeight+1&&r.left>=0&&r.right<=innerWidth+1&&fields.every(fits)&&buttons.every(fits)&&d.scrollWidth<=d.clientWidth+1,width:innerWidth,height:innerHeight,dialogHeight:r?.height,fields:fields.length};})()"#;
