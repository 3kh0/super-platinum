//! Native acceptance of automatic member acquisition, socket ACKs and frozen editor custody.
use super::member_freshness as recovery;
use super::*;
use crate::slack_links_check::wait_dom as native_wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_NONCE")
        .map_err(|_| "Missing profile member recovery readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid profile member recovery readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute profile member recovery readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("profile-member-freshness-{nonce}").as_str())
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
    eprintln!("profile-member-freshness-waiting: {expected}");
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
        eprintln!("profile-member-freshness-check: {result}");
    });
}
mod scenario;
use scenario::run;

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
    let script = format!("((requireFeedback)=>{LAYOUT})({})", false);
    let mut geometry = wait_layout(desktop, stage, &script).await?;
    let model = recovery::snapshot(&state.peek());
    let retired_without_write = stage == "account-replaced"
        && proof["replacement"]["actualInstallLoaded"] == true
        && proof["replacement"]["before"]["profile"]["enabled"] == true
        && proof["replacement"]["before"]["profile"]["attempts"] == json!([])
        && proof["replacement"]["before"]["profile"]["reads"] == 0
        && model["profile"]["enabled"] == false
        && model["profile"].get("attempts").is_none()
        && state.peek().profile_edit.sections.fixture.is_none();
    if model["profile"]["attempts"] != json!([]) && !retired_without_write {
        return Err("Automatic member refresh dispatched a section write".into());
    }
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
        "profile-member-freshness-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "profile-member-freshness-capture: {}",
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
