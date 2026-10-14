//! Native inline-modal checks; fixture records single-shot requests without transport.
use super::users_fixture as fixture;
use super::*;
use serde_json::json;
mod channel;
mod conversation;
mod multi_channel;
mod multi_conversation;
mod single;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let single = fixture::single_enabled();
        let conversation = super::conversation_fixture::enabled();
        let channel = super::channel_fixture::enabled();
        let multi_channel = super::multi_channel_fixture::enabled();
        let multi_conversation = super::multi_conversation_fixture::enabled();
        let result = match if multi_conversation {
            multi_conversation::run(state, &desktop).await
        } else if multi_channel {
            multi_channel::run(state, &desktop).await
        } else if channel {
            channel::run(state, &desktop).await
        } else if conversation {
            conversation::run(state, &desktop).await
        } else if single {
            single::run(state, &desktop).await
        } else {
            run(state, &desktop).await
        } {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        let prefix = if multi_conversation {
            "app-modal-multi-conversation-check"
        } else if multi_channel {
            "app-modal-multi-channel-check"
        } else if channel {
            "app-modal-channel-check"
        } else if conversation {
            "app-modal-conversation-check"
        } else if single {
            "app-modal-user-check"
        } else {
            "app-modal-users-check"
        };
        eprintln!("{prefix}: {result}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(
        desktop,
        r#"({ok:document.querySelectorAll('.app-modal-users').length===2})"#,
    )
    .await?;
    if state.peek().core.transport.is_some() {
        return Err("Expected disconnected fixture".into());
    }
    let initial = state
        .peek()
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .submission()
        .map_err(|e| e.to_string())?;
    if initial["values"]["reviewers"]["value"]["selected_users"] != json!(["U00000001"])
        || initial["values"]["optional"]["value"]["selected_users"] != json!([])
    {
        return Err("Initial/empty modal state mismatch".into());
    }
    input(desktop, "note", "Local sibling draft").await?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-user-id="U00000001"]')&&!document.querySelector('[data-user-id="U00000006"]')&&!document.querySelector('[data-user-id="U00000007"]')})"#).await?;
    click(desktop, "[data-remove-user='U00000001']").await?;
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait(desktop,r#"({ok:document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent.includes('required')})"#).await?;
    attempts(state, 0)?;
    for id in ["U00000001", "U00000002", "U00000003"] {
        click(desktop, &format!("[data-user-id='{id}']")).await?;
    }
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-remove-user]').length===3&&document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent.trim().length>0})"#).await?;
    attempts(state, 0)?;
    let invalid = capture(desktop, "maximum").await?;
    eval(desktop,r#"(()=>{document.querySelector('[data-block-id="reviewers"] .app-modal-error').scrollIntoView({block:'center'});return {ok:true};})()"#).await?;
    let validation_geometry = wait(desktop, VALIDATION_LAYOUT).await?;
    let validation = capture(desktop, "validation").await?;
    eval(desktop,r#"(()=>{document.querySelector('[data-block-id="reviewers"] input').scrollIntoView({block:'center'});return {ok:true};})()"#).await?;
    click(desktop, "[data-remove-user='U00000003']").await?;
    input(desktop, "reviewers", "Blair").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-user-id]').length===1&&document.querySelector('[data-user-id="U00000002"]')?.getAttribute('aria-checked')==='true'&&document.querySelectorAll('[data-remove-user]').length===2})"#).await?;
    eval(desktop,r#"(()=>{window.__oldModalUser=document.querySelector('[data-user-id="U00000002"]');return {ok:true};})()"#).await?;
    let mut update = fixture::view();
    update["blocks"][1]["element"]["initial_users"] = json!(["U00000004"]);
    update["blocks"][2]["element"]["initial_users"] = json!(["U00000004"]);
    super::apply_fixture_update(state, update);
    wait(desktop,r#"({ok:!document.querySelector('[data-user-id]')&&document.querySelector('[data-block-id="optional"] [data-remove-user="U00000004"]')&&document.querySelector('[data-block-id="note"] input')?.value==='Local sibling draft'})"#).await?;
    eval(
        desktop,
        "(()=>{window.__oldModalUser.click();return {ok:true};})()",
    )
    .await?;
    click(
        desktop,
        "[data-block-id='optional'] [data-remove-user='U00000004']",
    )
    .await?;
    open(desktop, "reviewers").await?;
    wait(desktop,r#"({ok:document.querySelectorAll('[data-remove-user]').length===2&&document.querySelector('[data-user-id="U00000002"]')?.getAttribute('aria-checked')==='true'})"#).await?;
    let geometry = eval(desktop, LAYOUT).await?;
    if geometry["ok"] != true {
        return Err(format!("Inline selector geometry: {geometry}"));
    }
    let selected = capture(desktop, "selected").await?;
    // Collapse only the inline choices with Escape; do not close the app's view.
    wait(
        desktop,
        r#"({ok:!!document.querySelector('[data-user-id]')?.closest('[data-escape-local]')})"#,
    )
    .await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-user-id]');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:!!document.getElementById('app-modal-form')&&!document.querySelector('[data-user-id]')&&!document.querySelector('.app-modal-users[data-escape-local]')&&document.activeElement===document.querySelector('[data-block-id="reviewers"] input')})"#).await?;
    open(desktop, "reviewers").await?;
    eval(desktop,r#"(()=>{const e=document.querySelector('[data-block-id="reviewers"] .app-modal-users-actions button');e.focus();e.click();return {ok:true};})()"#).await?;
    wait(desktop,r#"({ok:!document.querySelector('[data-user-id]')&&document.activeElement===document.querySelector('[data-block-id="reviewers"] input')})"#).await?;
    eval(desktop,r#"(()=>{const b=document.querySelector('#app-modal-form button[type=submit]');b.click();b.click();return {ok:true};})()"#).await?;
    wait_attempts(state, 1).await?;
    wire(state, 0, &["U00000001", "U00000002"])?;
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&[...document.querySelectorAll('.app-modal-users input,.app-modal-users button')].every(e=>e.disabled)})"#).await?;
    fixture::complete(state, false);
    wait(desktop,r#"({ok:document.querySelector('[data-block-id="reviewers"] .app-modal-error')?.textContent==='Choose another reviewer.'&&!document.querySelector('#app-modal-form button[type=submit]').disabled})"#).await?;
    click(desktop, "[data-remove-user='U00000002']").await?;
    click(desktop, "#app-modal-form button[type=submit]").await?;
    wait_attempts(state, 2).await?;
    wire(state, 1, &["U00000001"])?;
    fixture::complete(state, true);
    wait(desktop,r#"({ok:document.querySelector('#app-modal-form button[type=submit]')?.disabled&&document.querySelector('.app-modal-notice')?.textContent.includes('not be repeated')})"#).await?;
    super::submit(state);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    attempts(state, 2)?;
    eval(
        desktop,
        "(()=>{document.querySelector('.app-modal-body').scrollTop=0;return {ok:true};})()",
    )
    .await?;
    let uncertain = capture(desktop, "uncertain").await?;
    let mut receipt = geometry;
    receipt["captures"] = json!({"maximum":invalid,"validation":validation,"selected":selected,"uncertain":uncertain});
    receipt["validationErrorVisible"] = validation_geometry["ok"].clone();
    receipt["validationGeometry"] = validation_geometry;
    receipt["preparedRequests"] = json!(2);
    for flag in [
        "initialAndEmptyState",
        "selfAndEligibility",
        "requiredAndMaximum",
        "inlineEditsNoDispatch",
        "searchRetainsSelection",
        "remoteRetainsDirtyAndRefreshesPristine",
        "staleChoiceRejected",
        "escapeRetainsModal",
        "fullSiblingState",
        "pendingDuplicateRejected",
        "serverFieldErrorRetained",
        "uncertainNoReplay",
        "noTransport",
    ] {
        receipt[flag] = json!(true);
    }
    Ok(receipt)
}
fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let shell = state.peek();
    let s = shell.app_modal.as_ref().ok_or("Missing modal")?;
    if s.fixture_attempts == expected
        && s.users_fixture
            .as_ref()
            .is_some_and(|f| f.states.len() == expected && f.valid)
        && shell.core.transport.is_none()
    {
        Ok(())
    } else {
        Err(format!("Expected {expected} valid prepared requests"))
    }
}
fn wire(state: Signal<ShellState>, index: usize, users: &[&str]) -> Result<(), String> {
    let shell = state.peek();
    let f = shell
        .app_modal
        .as_ref()
        .unwrap()
        .users_fixture
        .as_ref()
        .unwrap();
    let state = &f.states[index];
    if state
        == &json!({"values":{"note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}},
        "reviewers":{"value":{"type":"multi_users_select","selected_users":users}},"optional":{"value":{"type":"multi_users_select","selected_users":[]}}}})
    {
        Ok(())
    } else {
        Err("Prepared full state differs from current modal fields".into())
    }
}
async fn wait_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..40 {
        if attempts(state, expected).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    attempts(state, expected)
}
async fn open(d: &dioxus::desktop::DesktopContext, block: &str) -> Result<(), String> {
    let q = serde_json::to_string(&format!("[data-block-id='{block}'] input")).unwrap();
    let opened=eval(d,&format!("(()=>{{const e=document.querySelector({q});if(!e||e.disabled)return {{ok:false,modal:!!document.getElementById('app-modal-form')}};e.scrollIntoView({{block:'center'}});e.focus();e.click();return {{ok:true}};}})()")).await?;
    if opened["ok"] != true {
        return Err(format!("Could not open modal users {block}: {opened}"));
    }
    wait(d,&format!("(()=>{{const e=document.querySelector({q});return {{ok:!!e?.closest('.app-modal-users')?.querySelector('[data-entity-id]'),modal:!!document.getElementById('app-modal-form'),query:e?.value,expanded:e?.getAttribute('aria-expanded')}};}})()")).await?;
    Ok(())
}
async fn input(
    d: &dioxus::desktop::DesktopContext,
    block: &str,
    value: &str,
) -> Result<(), String> {
    let q = serde_json::to_string(&format!("[data-block-id='{block}'] input")).unwrap();
    let v = serde_json::to_string(value).unwrap();
    eval(d,&format!("(()=>{{const e=document.querySelector({q});e.value={v};e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()")).await?;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    Ok(())
}
async fn click(d: &dioxus::desktop::DesktopContext, q: &str) -> Result<(), String> {
    let q = serde_json::to_string(q).unwrap();
    let v=eval(d,&format!("(()=>{{const e=document.querySelector({q});if(!e||e.disabled)return {{ok:false}};e.click();return {{ok:true}};}})()")).await?;
    if v["ok"] != true {
        return Err(format!("Missing or disabled control {q}"));
    }
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    Ok(())
}
async fn wait(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(d, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "Modal users did not settle: {last}; assertion: {script}"
    ))
}
async fn eval(d: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    d.webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation request failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Native evaluation timed out")?
        .ok_or("Callback closed")?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| "Invalid callback")?;
    match v {
        Value::String(v) => serde_json::from_str(&v).map_err(|_| "Invalid encoded callback".into()),
        v => Ok(v),
    }
}
async fn capture(
    d: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Option<String>, String> {
    let Some(directory) = std::env::var_os("SUPER_PLATINUM_APP_MODAL_USERS_CAPTURE_DIR") else {
        return Ok(None);
    };
    if !fixture::enabled() {
        return Err("Capture requires modal users fixture".into());
    }
    eval(d,"(()=>{window.__modalUserPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__modalUserPaint=true));return {ok:true};})()").await?;
    wait(d, "({ok:window.__modalUserPaint===true})").await?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    let path = std::path::PathBuf::from(directory).join(format!("{name}-{stage}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
const LAYOUT: &str = r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),body=d.querySelector('.app-modal-body'),input=d.querySelector('[data-block-id="reviewers"] input'),footer=d.querySelector('footer');input.scrollIntoView({block:'center'});const r=d.getBoundingClientRect(),ir=input.getBoundingClientRect(),fr=footer.getBoundingClientRect();const noOverflow=r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;const eachVisible=ir.top>=r.top&&ir.bottom<=fr.top&&fr.bottom<=innerHeight;const hit=document.elementFromPoint(ir.left+ir.width/2,ir.top+ir.height/2),hitTestedControls=hit===input;const keyboardFocused=document.activeElement===input;return {ok:noOverflow&&eachVisible&&hitTestedControls&&keyboardFocused,noOverflow,eachVisible,hitTestedControls,keyboardFocused,viewport:[innerWidth,innerHeight]};})()"#;
const VALIDATION_LAYOUT: &str = r#"(()=>{const e=document.querySelector('[data-block-id="reviewers"] .app-modal-error'),body=e?.closest('.app-modal-body'),footer=document.querySelector('#app-modal-form footer');if(!e||!body||!footer)return {ok:false};const r=e.getBoundingClientRect(),b=body.getBoundingClientRect(),f=footer.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return {ok:e.textContent.trim().length>0&&r.width>0&&r.height>0&&r.left>=Math.max(b.left,0)&&r.right<=Math.min(b.right,innerWidth)&&r.top>=Math.max(b.top,0)&&r.bottom<=Math.min(b.bottom,f.top,innerHeight)&&(hit===e||e.contains(hit)),error:[r.left,r.top,r.right,r.bottom],scrollViewport:[b.left,b.top,b.right,b.bottom],viewport:[innerWidth,innerHeight]};})()"#;
