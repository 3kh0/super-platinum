//! Real Profile selector hits drive the closed shared conditional refresh adapter.
use super::recovery::{self, FLAG};
use super::*;
use crate::app_home::channel_fetches::conditional::fixture as transport;
use crate::slack_links_check::wait_dom as native_wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_NONCE")
        .map_err(|_| "Missing profile channel recovery readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid profile channel recovery readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute profile channel recovery readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("profile-channel-recovery-{nonce}").as_str())
    {
        return Err("Wrong profile channel recovery readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect profile channel recovery readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid profile channel recovery readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => {
                    result.map_err(|_| "Inspect profile channel recovery readiness marker")?
                }
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid profile channel recovery readiness marker".into());
            }
            let bytes = std::fs::read(path)
                .map_err(|_| "Read profile channel recovery readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse profile channel recovery readiness marker".into())
        })
        .await
        .map_err(|_| "Profile channel recovery readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Profile channel recovery readiness marker must be fresh".into());
    }
    eprintln!("profile-channel-recovery-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong profile channel recovery readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Profile channel recovery readiness timed out".into())
}

pub(crate) fn start(mut state: Signal<ShellState>, desktop: DesktopContext) {
    if !recovery::named() || std::env::var(FLAG).as_deref() != Ok("1") {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"error":e,"model":recovery::snapshot(&mut state.write())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Profile channel recovery timed out"})
            }
        };
        eprintln!("profile-channel-recovery-check: {result}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    recovery::seed(state).await?;
    let mut requests = Vec::new();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    click(desktop, "Edit description", false).await?;
    editor(desktop, "Working with ", "#project-harbor", true).await?;
    // A first canonical hit may need freshness. Establish it through a real
    // conditional unchanged response, never a direct freshness grant.
    let initial = request(state, &mut requests).await?;
    release(state, initial, Ok(unchanged())).await?;
    edit(desktop, recovery::DRAFT).await?;
    use_description(state, desktop, recovery::DRAFT).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, "#project-harbor", true).await?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"initialConditionalRead":true}),
        &mut stages,
        &mut captures,
    )
    .await?;

    let stale = reopen_stale(state, desktop, "#project-harbor").await?;
    let changed = request(state, &mut requests).await?;
    capture(
        state,
        desktop,
        name,
        "opened-stale",
        stale,
        &mut stages,
        &mut captures,
    )
    .await?;
    release(state, changed, Ok(changed_response(recovery::RENAMED, 102))).await?;
    editor(desktop, recovery::DRAFT, "#project-harbor", true).await?;
    require_name(state, recovery::RENAMED, 102)?;
    capture(
        state,
        desktop,
        name,
        "renamed-mounted",
        json!({"mountedLabelRetained":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, "#recovered-harbor", true).await?;
    capture(
        state,
        desktop,
        name,
        "reopened",
        json!({"newCanonicalLabel":true}),
        &mut stages,
        &mut captures,
    )
    .await?;

    for (stage, response) in [
        ("unchanged", Ok(unchanged())),
        (
            "failure-retained",
            Err(super_platinum_core::slack::Error::Api(
                "synthetic_conditional_failure".into(),
            )),
        ),
        (
            "malformed-retained",
            Ok(json!({"results":"not-an-array","member_channels":[]})),
        ),
    ] {
        let before = reopen_stale(state, desktop, "#recovered-harbor").await?;
        let serial = request(state, &mut requests).await?;
        if stage == "unchanged" {
            release(state, serial, response).await?;
        } else {
            if !transport::release(&mut state.write(), serial, response) {
                return Err("Directory fallback trigger was not delivered".into());
            }
            let fallback =
                request_method(state, &mut requests, "conversations.genericInfo").await?;
            let response = if stage == "malformed-retained" {
                Ok(json!({"ok":true,"channels":"bad"}))
            } else {
                Err(super_platinum_core::slack::Error::Api(
                    "synthetic_fallback_failure".into(),
                ))
            };
            release(state, fallback, response).await?;
        }
        require_name(state, recovery::RENAMED, 102)?;
        editor(desktop, recovery::DRAFT, "#recovered-harbor", true).await?;
        capture(state,desktop,name,stage,json!({"predecessorRetained":true,"fallbackAttempted":stage!="unchanged","topic":before}),&mut stages,&mut captures).await?;
    }

    reopen_stale(state, desktop, "#recovered-harbor").await?;
    let serial = request(state, &mut requests).await?;
    let socket = recovery::retire_socket(state)?;
    if !transport::release(
        &mut state.write(),
        serial,
        Ok(changed_response("retired-socket-name", 999)),
    ) {
        return Err("Retired socket response not delivered".into());
    }
    // Source reconnect finally requeues attempted IDs under the new accepted
    // connection. Refusing the old publication does not discard that retry.
    let successor = request(state, &mut requests).await?;
    release(state, successor, Ok(unchanged())).await?;
    require_name(state, recovery::RENAMED, 102)?;
    editor(desktop, recovery::DRAFT, "#recovered-harbor", true).await?;
    capture(
        state,
        desktop,
        name,
        "socket-retired",
        json!({"latePublicationRefused":true,"successorRead":true,"socket":socket}),
        &mut stages,
        &mut captures,
    )
    .await?;

    transport::hold_drain(&mut state.write()).map_err(|e| format!("Hold source drain: {e}"))?;
    reopen_stale(state, desktop, "#recovered-harbor").await?;
    let serial = request(state, &mut requests).await?;
    if !transport::release(
        &mut state.write(),
        serial,
        Ok(changed_response(recovery::QUEUED, 103)),
    ) {
        return Err("Pending conditional rename release refused".into());
    }
    wait_pending(state).await?;
    let queued = recovery::snapshot(&mut state.write());
    if !queued["source"].is_null() || queued["nativeName"] != recovery::RENAMED {
        return Err(format!(
            "Pending conditional source exposed as ready: {queued}"
        ));
    }
    editor(desktop, recovery::DRAFT, "#recovered-harbor", true).await?;
    capture(
        state,
        desktop,
        name,
        "conditional-pending",
        json!({"conditionalQueued":true,"canonicalUnavailable":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    transport::resynced(state, vec![mentions::CHANNEL.into()])
        .map_err(|e| format!("Pending topic resync: {e}"))?;
    let stale = recovery::snapshot(&mut state.write());
    recovery::socket_rename(state)?;
    let socket = recovery::snapshot(&mut state.write());
    if !socket["source"].is_null()
        || socket["nativeName"] != recovery::SOCKET
        || !transport::pending(&state.peek()).is_empty()
    {
        return Err(format!("Pending socket source order failed: {socket}"));
    }
    editor(desktop, recovery::DRAFT, "#recovered-harbor", true).await?;
    capture(
        state,
        desktop,
        name,
        "socket-pending",
        json!({"topic":stale,"socketAccepted":true,"canonicalUnavailable":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    transport::release_drain(state).map_err(|e| format!("Release source drain: {e}"))?;
    wait_idle(state).await?;
    require_name(state, recovery::SOCKET, 103)?;
    click(desktop, "Cancel description edit", false).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, "#socket-latest-harbor", true).await?;
    // Read admission is asynchronous. Observe a bounded quiet interval and the
    // actual freshness/worker state, not only an empty held-response inventory.
    for _ in 0..10 {
        let current = transport::status(&state.peek());
        let fresh = current["managers"].as_array().is_some_and(|rows| {
            !rows.is_empty()
                && rows.iter().all(|r| {
                    r["worker"] == false
                        && r["active"] == false
                        && r["pendingIds"] == json!([])
                        && r["freshIds"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|id| id == mentions::CHANNEL))
                })
        });
        if current["requestCount"] != queued["conditional"]["requestCount"]
            || !transport::idle(&state.peek())
            || !fresh
        {
            return Err(format!(
                "Reopening latest source scheduled a redundant read: {current}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    capture(state, desktop, name, "pending-reopened", json!({"latestSocketLabel":true,"noRedundantRead":true,"requestCount":queued["conditional"]["requestCount"]}), &mut stages, &mut captures).await?;

    reopen_stale(state, desktop, "#socket-latest-harbor").await?;
    let serial = request(state, &mut requests).await?;
    let account = mentions::replace_account(state)?;
    release(
        state,
        serial,
        Ok(changed_response("retired-account-name", 1000)),
    )
    .await?;
    let after = recovery::snapshot(&mut state.write());
    if !after["source"].is_null()
        || !after["nativeName"].is_null()
        || after["profile"] != json!({"enabled":false})
    {
        return Err(format!(
            "Retired account published into replacement: {after}"
        ));
    }
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})"#,
    )
    .await?;
    capture(
        state,
        desktop,
        name,
        "account-replaced",
        account,
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"readiness":ready,"stages":stages,"captures":captures,
        "requests":requests,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,
        "transport":"closed entered requests; no HTTP polling","visualReview":"PNG inspection required separately"}),
    )
}
fn unchanged() -> Value {
    json!({"results":[],"member_channels":[]})
}
fn changed_response(name: &str, updated: u64) -> Value {
    json!({"results":[recovery::row(name,updated)],"member_channels":[mentions::CHANNEL]})
}
async fn request(state: Signal<ShellState>, requests: &mut Vec<Value>) -> Result<u64, String> {
    request_method(state, requests, "channels.info").await
}
async fn request_method(
    mut state: Signal<ShellState>,
    requests: &mut Vec<Value>,
    method: &str,
) -> Result<u64, String> {
    for _ in 0..100 {
        let pending = transport::pending(&state.peek());
        if pending.len() > 1 {
            return Err("Duplicate conditional request admission".into());
        }
        if let Some(p) = pending.first() {
            if p.ids != [mentions::CHANNEL] || p.method != method {
                return Err(format!(
                    "Conditional request escaped exact referenced channel/{method}"
                ));
            }
            let predecessor = recovery::snapshot(&mut state.write());
            requests.push(
                json!({"serial":p.serial,"method":p.method,"ids":p.ids,"versions":p.versions,
                "predecessor":predecessor}),
            );
            return Ok(p.serial);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Conditional {method} request not admitted after actual selector hit: {}",
        recovery::snapshot(&mut state.write())
    ))
}
async fn release(
    mut state: Signal<ShellState>,
    serial: u64,
    response: Result<Value, super_platinum_core::slack::Error>,
) -> Result<(), String> {
    if !transport::release(&mut state.write(), serial, response) {
        return Err(format!(
            "Held conditional response {serial} refused release"
        ));
    }
    wait_idle(state).await
}
async fn wait_idle(mut state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..100 {
        if transport::idle(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Conditional settlement did not finish: {}",
        recovery::snapshot(&mut state.write())
    ))
}
async fn wait_pending(mut state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..100 {
        let status = transport::status(&state.peek());
        if status["drainHeld"] == true
            && status["held"] == 0
            && status["managers"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r["worker"] == false))
            && status["queues"].as_array().is_some_and(|rows| {
                rows.iter().any(|r| {
                    r["pending"] == true
                        && r["incoming"].as_array().is_some_and(|rows| {
                            rows.iter().any(|r| {
                                r["id"] == mentions::CHANNEL
                                    && r["name"] == recovery::QUEUED
                                    && r["updated"] == 103
                            })
                        })
                })
            })
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Conditional rename did not settle into held source queue: {}",
        recovery::snapshot(&mut state.write())
    ))
}
async fn reopen_stale(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    label: &str,
) -> Result<Value, String> {
    click(desktop, "Cancel description edit", false).await?;
    let before = recovery::snapshot(&mut state.write());
    transport::resynced(state, vec![mentions::CHANNEL.into()])
        .map_err(|e| format!("Actual topic resync: {e}"))?;
    let after = recovery::snapshot(&mut state.write());
    if before["source"] != after["source"]
        || before["nativeName"] != after["nativeName"]
        || !transport::pending(&state.peek()).is_empty()
    {
        return Err(format!(
            "Topic erased predecessor or fetched without selector: before={before}, after={after}"
        ));
    }
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, label, true).await?;
    Ok(json!({"topicRetainedPredecessor":true,"topicDidNotFetch":true}))
}
fn require_name(mut state: Signal<ShellState>, name: &str, version: u64) -> Result<(), String> {
    let v = recovery::snapshot(&mut state.write());
    if v["source"]["name"] != name || v["source"]["updated"] != version || v["nativeName"] != name {
        return Err(format!("Conditional raw/native publication mismatch: {v}"));
    }
    Ok(())
}
async fn click(desktop: &DesktopContext, label: &str, twice: bool) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==={});if(!b||b.disabled)return{{ok:false}};b.scrollIntoView({{block:'nearest'}});const r=b.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==b&&!b.contains(hit))return{{ok:false}};b.click();if({twice})b.click();return{{ok:true}};}})()"#,json!(label))).await.map(|_|()).map_err(|e|format!("Click {label}: {e}"))
}
async fn editor(
    desktop: &DesktopContext,
    text: &str,
    label: &str,
    editable: bool,
) -> Result<(), String> {
    let label_json = json!(label);
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);return{{ok:!!v&&e.isContentEditable==={editable}&&v.snapshot.blocks[0].elements[0].elements[0].text==={}&&v.tokens.length===2&&v.tokens.every(t=>t.label==={label_json})&&v.tokens.map(t=>t.value.fixture_occurrence).join(',')==='1,2'}};}})()"#,json!(text))).await.map(|_|()).map_err(|e|format!("Editor content (prefix {:?}, length {}, editable {editable}): {e}", text.chars().take(32).collect::<String>(), text.len()))
}
async fn edit(desktop: &DesktopContext, text: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');if(!e?.isContentEditable||!leaf)return{{ok:false}};leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return{{ok:true}};}})()"#,json!(text))).await?;
    editor(desktop, text, "#project-harbor", true).await
}
async fn use_description(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
) -> Result<(), String> {
    click(desktop, "Use description", false).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})"#).await?;
    if mentions::snapshot(&state.peek())["blocks"] != json!(mentions::blocks(text)) {
        return Err("Accepted description changed opaque tokens".into());
    }
    Ok(())
}
/// Each poll returns an owned diagnostic snapshot even while its condition is
/// false, rather than losing the exact DOM generation behind a generic timeout.
async fn wait_dom(desktop: &DesktopContext, script: &str) -> Result<Value, String> {
    let wrapped = format!(
        r#"(()=>{{let measured;try{{measured=({script});}}catch(error){{measured={{ok:false,error:String(error)}};}}
      const d=document.getElementById('shell-overlay-dialog'),e=document.getElementById('profile-section-rich');
      return{{ok:true,measured,diagnostic:{{dialog:!!d,visit:d?.dataset.dialogGeneration,serial:d?.dataset.sectionSerial,locked:d?.dataset.sectionLocked,
        richPane:!!document.getElementById('profile-section-rich-pane'),richEditor:!!e,richEditable:e?.isContentEditable,
        text:e?.textContent?.slice(0,100),tokens:e?[...e.querySelectorAll('.rich-editor-token')].map(t=>t.textContent):[],
        buttons:d?[...d.querySelectorAll('button')].map(b=>({{text:b.textContent,disabled:b.disabled}})):[],
        notices:d?[...d.querySelectorAll('.profile-edit-notice,.profile-edit-field-error,#profile-section-rich-status')].map(n=>n.textContent):[],
        choice:document.getElementById('profile-section-field-choice')?.value,
        country:document.querySelector('select[aria-label="Country (optional)"]')?.value}}}};}})()"#
    );
    let mut last = Value::Null;
    for _ in 0..60 {
        last = native_wait_dom(desktop, &wrapped).await?;
        if last["measured"]["ok"] == true {
            return Ok(last["measured"].clone());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Profile DOM condition timed out; last measurement: {last}"
    ))
}

async fn capture(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let script = format!(
        "((requireFeedback)=>{LAYOUT})({})",
        matches!(stage, "failure-retained" | "malformed-retained")
    );
    let mut geometry = wait_layout(desktop, stage, &script).await?;
    let model = recovery::snapshot(&mut state.write());
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture path worker failed")?
        .map_err(|_| "Capture path inspection failed")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "profile-channel-recovery-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "profile-channel-recovery-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn wait_layout(desktop: &DesktopContext, stage: &str, script: &str) -> Result<Value, String> {
    let wrapped = format!("(()=>({{ok:true,measured:({script})}}))()");
    let mut last = Value::Null;
    for _ in 0..60 {
        last = wait_dom(desktop, &wrapped).await?["measured"].clone();
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Layout {stage} timed out; last measurement: {last}"
    ))
}
const LAYOUT: &str = r#"(()=>{
 const d=document.querySelector('.profile-edit'),body=d?.querySelector('.profile-edit-body'),e=document.getElementById('profile-section-rich');
 if(body)body.scrollTop=body.scrollHeight;if(e)e.scrollTop=e.scrollHeight;
 if(!e)document.getElementById('profile-section-field-long')?.closest('.profile-edit-field')?.scrollIntoView({block:'nearest'});
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const s=getComputedStyle(e),fragments=[...e.getClientRects()].filter(r=>r.width>0&&r.height>0);return s.visibility==='visible'&&s.display!=='none'&&fragments.length>0&&fragments.every(r=>{const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);let unclipped=true;for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=rect(p);if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))unclipped=false;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))unclipped=false;}return unclipped&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&(hit===e||e.contains(hit));});};
 const controls=d?[...d.querySelectorAll('footer button,#profile-section-rich-pane > button')].map(e=>({text:e.textContent,disabled:e.disabled,visible:visible(e),rect:rect(e)})):[];
 const notices=d?[...d.querySelectorAll('.profile-edit-notice,.profile-edit-field-error,#profile-section-rich-status')].filter(e=>e.textContent.trim()).map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)})):[];
 const feedbackVisible=notices.some(n=>n.text==='Channel metadata could not refresh.'&&n.visible);
 const t=document.getElementById('status-toast'),dismiss=t?.querySelector('.toast-dismiss');
 const globalToast=t?{text:t.querySelector('.toast-text')?.textContent??'',positioned:t.dataset.positioned==='true',visibility:getComputedStyle(t).visibility,visible:visible(t),rect:rect(t),dismiss:dismiss?{visible:visible(dismiss),rect:rect(dismiss)}:null}:null;
 const tokens=e?[...e.querySelectorAll('.rich-editor-token')].map(e=>({text:e.textContent,editable:e.isContentEditable,visible:visible(e),rect:rect(e),fragments:[...e.getClientRects()].map(r=>({left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}))})):[];
 const native=e&&window.superPlatinumRichEditor.resolved(e),locked=document.getElementById('profile-section-field-locked');
 const noOverflow=document.documentElement.scrollWidth<=innerWidth+1&&(!d||d.scrollWidth<=d.clientWidth+1);
 return{ok:noOverflow&&(!d||visible(d))&&controls.every(e=>e.visible)&&notices.every(e=>e.visible)&&tokens.every(e=>e.visible)&&(!requireFeedback||feedbackVisible),
 viewport:[innerWidth,innerHeight],noOverflow,dialog:!!d,dialogRect:d?rect(d):null,controls,notices,tokens,feedbackRequired:requireFeedback,feedbackVisible,globalToast,
 captured:native||null,richEditable:e?e.isContentEditable:null,managedFieldPreserved:locked?locked.disabled&&locked.value==='Preserved organization value':null,
 longDisabled:document.getElementById('profile-section-field-long')?.disabled??null,
 choiceValue:document.getElementById('profile-section-field-choice')?.value??null,
 countryValue:document.querySelector('select[aria-label="Country (optional)"]')?.value??null};
})()"#;
