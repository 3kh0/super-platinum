//! Native acceptance of actual absent-member acquisition and Profile continuation.
use super::member_known::{self as recovery, FLAG};
use super::*;
use crate::slack_links_check::wait_dom as native_wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_NONCE")
        .map_err(|_| "Missing profile member recovery readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid profile member recovery readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute profile member recovery readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("profile-member-known-{nonce}").as_str())
    {
        return Err("Wrong profile member recovery readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect profile member recovery readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid profile member recovery readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect profile member recovery readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid profile member recovery readiness marker".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "Read profile member recovery readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse profile member recovery readiness marker".into())
        })
        .await
        .map_err(|_| "Profile member recovery readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Profile member recovery readiness marker must be fresh".into());
    }
    eprintln!("profile-member-known-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong profile member recovery readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Profile member recovery readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !recovery::enabled() {
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
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":recovery::case(),"error":error,"model":recovery::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Member recovery timed out","model":recovery::snapshot(&state.peek())})
            }
        };
        eprintln!("profile-member-known-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    let case = recovery::case();
    if !matches!(case.as_str(), "stale" | "terminal" | "account") {
        return Err("Unknown known-member case".into());
    }
    recovery::seed(state).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    let original = recovery::snapshot(&state.peek());
    retained(state, &original)?;
    if original["sourcePresent"] != true
        || original["source"]["updated"] != json!(if case == "terminal" { 0 } else { 101 })
        || (original["source"]["isNonExistent"] == true) != (case == "terminal")
    {
        return Err(format!("Wrong actual initial source: {original}"));
    }
    form(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"initialRawKnown":true,"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    unavailable(desktop).await?;
    // Polling the actual unavailable UI allows ordinary opening's async path to
    // run; no source-known target may enter the missing-member transport.
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !recovery::requests().is_empty() || finished(state) != 0 {
        return Err("Ordinary known opening forced a read".into());
    }
    retained(state, &original)?;
    capture(
        state,
        desktop,
        name,
        "ordinary-open",
        json!({"noForcedRead":true,"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Retry description", false).await?;
    let first = request(state).await?;
    loading(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "held",
        json!({"explicitRetryRead":true,"duplicateIdsCoalesced":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "account" {
        let proof = recovery::replace_account(state);
        let before = finished(state);
        recovery::release(first, Ok((recovery::success("TINIT0001"), None)))?;
        settled(state, before).await?;
        wait_dom(desktop, "({ok:!document.querySelector('.profile-edit')})").await?;
        let after = recovery::snapshot(&state.peek());
        if after["sourcePresent"] == true
            || after["native"] != Value::Null
            || after["profile"]["enabled"] == true
        {
            return Err(format!("Retired owner adopted known member: {after}"));
        }
        capture(
            state,
            desktop,
            name,
            "account-replaced",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        let accepted = if case == "stale" {
            release_and_settle(
                state,
                first,
                Ok((json!({"ok":true,"results":"malformed"}), None)),
            )
            .await?;
            unavailable(desktop).await?;
            retained(state, &original)?;
            capture(
                state,
                desktop,
                name,
                "malformed",
                json!({"sourceRetained":true,"draftRetained":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Retry description", false).await?;
            let offline = request(state).await?;
            release_and_settle(
                state,
                offline,
                Err(Error::Offline("closed known fixture offline".into())),
            )
            .await?;
            unavailable(desktop).await?;
            retained(state, &original)?;
            capture(
                state,
                desktop,
                name,
                "offline",
                json!({"sourceRetained":true,"draftRetained":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            recovery::native_drift(&mut state.write())?;
            click(desktop, "Retry description", false).await?;
            let repair = request(state).await?;
            loading(desktop).await?;
            let drift = recovery::snapshot(&state.peek());
            if drift["source"] != original["source"]
                || drift["native"]["profile"]["display_name"] != "Native drift must be repaired"
            {
                return Err(format!("Native-only drift setup failed: {drift}"));
            }
            capture(
                state,
                desktop,
                name,
                "repair-held",
                json!({"sourceUnchanged":true,"nativeDriftBefore":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            repair
        } else {
            first
        };
        release_and_settle(state, accepted, Ok((success(state), None))).await?;
        editor(desktop).await?;
        let repaired = recovery::snapshot(&state.peek());
        if repaired["source"]["isNonExistent"] == true
            || repaired["source"]["updated"] != original["source"]["updated"]
            || repaired["native"]["profile"]["display_name"] != recovery::LABEL
            || repaired["invalidated"] != false
        {
            return Err(format!(
                "Known Retry failed same-version positive repair: {repaired}"
            ));
        }
        capture(state,desktop,name,"success",json!({"actualReadPageAndPrivateAdoption":true,"sameVersionPositive":true,"originalDraft":true}),&mut stages,&mut captures).await?;
        click(desktop, "Use description", false).await?;
        form(desktop).await?;
        if mentions::snapshot(&state.peek())["blocks"] != json!(recovery::blocks(recovery::DRAFT)) {
            return Err("Raw duplicate tokens changed".into());
        }
        let count = recovery::requests().len();
        click(desktop, "Edit description", false).await?;
        editor(desktop).await?;
        if recovery::requests().len() != count {
            return Err("Reopen replayed known read".into());
        }
        capture(
            state,
            desktop,
            name,
            "reopened",
            json!({"noRedundantRead":true,"originalDraft":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,"viewport":stages[0]["geometry"]["viewport"],
      "stages":stages,"captures":captures,"requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}
fn retained(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    let s = recovery::snapshot(&state.peek());
    if s["sourcePresent"] != true
        || s["source"] != original["source"]
        || s["native"] != original["native"]
        || s["interaction"] != original["interaction"]
        || s["disconnected"] != original["disconnected"]
        || s["profile"]["blocks"] != json!(recovery::blocks(recovery::DRAFT))
        || s["profile"]["attempts"] != json!([])
    {
        return Err(format!("Known source/native/draft custody changed: {s}"));
    }
    Ok(())
}
async fn loading(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Loading mention names. Your description is unchanged.'&&[...document.querySelectorAll('.profile-edit button')].some(b=>b.textContent==='Retry description'&&b.disabled)})"#).await.map(|_|())
}
fn success(state: Signal<ShellState>) -> Value {
    recovery::success(state.peek().core.active_team.as_deref().unwrap())
}
fn finished(state: Signal<ShellState>) -> u64 {
    recovery::snapshot(&state.peek())["driver"]["finished"]
        .as_u64()
        .unwrap_or(0)
}
async fn request(state: Signal<ShellState>) -> Result<u64, String> {
    for _ in 0..100 {
        let rows = recovery::pending();
        if rows.len() > 1 {
            return Err(format!("Unexpected parallel request: {rows:?}"));
        }
        if let Some(row) = rows.first() {
            let serial = row["serial"].as_u64().ok_or("Missing request serial")?;
            if row["kind"] == "discovery" {
                recovery::release(
                    serial,
                    Ok((
                        json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),"ttl_seconds":300}),
                        None,
                    )),
                )?;
            } else {
                if row["body"]
                    != json!({"ids":[recovery::MEMBER],"check_interaction":true,"include_profile_only_users":true})
                {
                    return Err(format!("Wrong actual member request: {row}"));
                }
                return Ok(serial);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Held member request timed out: {}",
        recovery::snapshot(&state.peek())
    ))
}
async fn release_and_settle(
    state: Signal<ShellState>,
    serial: u64,
    response: Result<(Value, Option<Vec<String>>), Error>,
) -> Result<(), String> {
    let before = finished(state);
    recovery::release(serial, response)?;
    settled(state, before).await
}
async fn settled(state: Signal<ShellState>, before: u64) -> Result<(), String> {
    for _ in 0..100 {
        let s = recovery::snapshot(&state.peek());
        if s["driver"]["finished"].as_u64().is_some_and(|n| n > before)
            && s["driver"]["active"] == 0
            && recovery::pending().is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Driver settlement timed out: {}",
        recovery::snapshot(&state.peek())
    ))
}
async fn form(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'})"#).await.map(|_|())
}
async fn editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);return{{ok:!!v&&e.isContentEditable&&v.snapshot.blocks[0].elements[0].elements[0].text==={}&&v.tokens.length===2&&v.tokens.every(t=>t.label==={})}};}})()"#,json!(recovery::DRAFT),json!(format!("@{}",recovery::LABEL)))).await.map(|_|())
}
async fn click(desktop: &DesktopContext, label: &str, twice: bool) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==={});if(!b||b.disabled)return{{ok:false}};b.scrollIntoView({{block:'nearest'}});const r=b.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==b&&!b.contains(hit))return{{ok:false}};b.click();if({twice})b.click();return{{ok:true}};}})()"#,json!(label))).await.map(|_|()).map_err(|e|format!("Click {label}: {e}"))
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
        matches!(
            stage,
            "ordinary-open" | "held" | "malformed" | "offline" | "repair-held"
        )
    );
    let mut geometry = wait_layout(desktop, stage, &script).await?;
    let model = recovery::snapshot(&state.peek());
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
        "profile-member-known-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "profile-member-known-capture: {}",
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
 const controls=d?[...d.querySelectorAll('footer button,#profile-section-rich-pane > button,#profile-section-field-long')].map(e=>({text:e.textContent,disabled:e.disabled,visible:visible(e),rect:rect(e)})):[];
 const notices=d?[...d.querySelectorAll('.profile-edit-notice,.profile-edit-field-error,#profile-section-rich-status')].filter(e=>e.textContent.trim()).map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)})):[];
 const feedbackVisible=notices.some(n=>(n.text==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'||n.text==='Loading mention names. Your description is unchanged.')&&n.visible);
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
