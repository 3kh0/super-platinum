use super::*;
pub(super) async fn form(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
pub(super) async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'})"#).await.map(|_|())
}
pub(super) async fn editor(desktop: &DesktopContext) -> Result<(), String> {
    editor_text(desktop, recovery::DRAFT).await
}
pub(super) async fn editor_text(desktop: &DesktopContext, text: &str) -> Result<(), String> {
    let labels = if recovery::mixed_absent::enabled() {
        json!([
            recovery::LABEL,
            recovery::LABEL,
            recovery::OTHER_LABEL,
            format!("@{}", super::super::member_recovery::LABEL),
            format!("@{}", super::super::member_recovery::LABEL),
            super::super::files::mounted_label(),
            super::super::files::mounted_label()
        ])
    } else if recovery::case() == "mixed-file-group" {
        json!([
            recovery::LABEL,
            recovery::LABEL,
            recovery::OTHER_LABEL,
            recovery::MEMBER_LABEL,
            super::super::files::mounted_label(),
            super::super::files::mounted_label()
        ])
    } else if recovery::case() == "mixed-member" {
        json!([
            recovery::LABEL,
            recovery::LABEL,
            recovery::OTHER_LABEL,
            recovery::MEMBER_LABEL
        ])
    } else if recovery::case() == "legacy" {
        json!([recovery::LABEL, recovery::LABEL])
    } else {
        json!([recovery::LABEL, recovery::LABEL, recovery::OTHER_LABEL])
    };
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);const stable=x=>JSON.stringify(x,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);return{{ok:!!v&&e.isContentEditable&&stable(v.snapshot.blocks)===stable({})&&JSON.stringify(v.tokens.map(t=>t.label))===JSON.stringify({labels})}};}})()"#,json!(recovery::blocks(text)))).await.map(|_|())
}
pub(super) async fn click(
    desktop: &DesktopContext,
    label: &str,
    twice: bool,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==={});if(!b||b.disabled)return{{ok:false}};b.scrollIntoView({{block:'nearest'}});const r=b.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==b&&!b.contains(hit))return{{ok:false}};b.click();if({twice})b.click();return{{ok:true}};}})()"#,json!(label))).await.map(|_|()).map_err(|e|format!("Click {label}: {e}"))
}
/// Each poll returns an owned diagnostic snapshot even while its condition is
/// false, rather than losing the exact DOM generation behind a generic timeout.
pub(super) async fn wait_dom(desktop: &DesktopContext, script: &str) -> Result<Value, String> {
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

pub(super) async fn capture(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let layout = if recovery::case() == "mixed-file-group"
        || recovery::mixed_absent::enabled()
        || recovery::known_freshness::enabled()
    {
        LAYOUT.replace("editable:e.isContentEditable,visible:visible(e)", "editable:e.isContentEditable,fontWeight:getComputedStyle(e).fontWeight,fontStyle:getComputedStyle(e).fontStyle,visible:visible(e)")
    } else {
        LAYOUT.to_string()
    };
    let script = format!(
        "((requireFeedback)=>{layout})({})",
        matches!(
            stage,
            "member-held"
                | "member-offline"
                | "retry-member-held"
                | "file-held"
                | "group-held"
                | "retry-file-held"
                | "ordinary-open"
                | "held"
                | "malformed"
                | "offline"
                | "partial"
                | "fallback-held"
                | "source-refused"
                | "pending"
                | "direct-held"
                | "limited"
                | "reopened-limited"
                | "expired"
        )
    );
    let mut geometry = wait_layout(desktop, stage, &script).await?;
    // Layout measurements can precede a composited WebView frame. Let the
    // already-checked state paint before taking the native screen capture.
    wait_dom(desktop, "(()=>{window.__profileGroupPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__profileGroupPaint=true));return {ok:true};})()").await?;
    wait_dom(desktop, "({ok:window.__profileGroupPaint===true})").await?;
    geometry["paintFramesObserved"] = json!(2);
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
        "profile-group-recovery-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "profile-group-recovery-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
pub(super) async fn wait_layout(
    desktop: &DesktopContext,
    stage: &str,
    script: &str,
) -> Result<Value, String> {
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
 const feedbackVisible=notices.some(n=>(n.text==='Loading reference details. Your description is unchanged.'||n.text==='Reference details are unavailable or changed while opening. Your description is unchanged. Retry to load document and mention details.'||n.text==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'||n.text==='Loading mention names. Your description is unchanged.'||n.text==='Slack asked us to wait before loading group names. Your description is unchanged.'||n.text==='You can retry loading group names now. Your description is unchanged.')&&n.visible);
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
