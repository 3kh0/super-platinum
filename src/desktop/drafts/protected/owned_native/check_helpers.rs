//! Fixture-only DOM observations and actual existing Send-button activation.
use super::*;
pub(super) fn label(target: ComposerTarget) -> &'static str {
    if target == ComposerTarget::Thread {
        "thread"
    } else {
        "channel"
    }
}
pub(super) fn ensure(value: bool, reason: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(reason.into()) }
}
pub(super) async fn eval(script: String) -> Result<Value, String> {
    let mut eval = document::eval(&script);
    let reply = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Value>()).await;
    let _ = eval.send(());
    match reply {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Protected DOM: {value}")),
        _ => Err("Protected DOM timeout".into()),
    }
}
pub(super) async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
pub(super) async fn dom(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await protectedDraftsDOM({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("check_dom.js"),json!(operation),json!({"target":label(target),"text":text}))).await
}
pub(super) async fn ready(
    state: Signal<ShellState>,
    target: ComposerTarget,
    previous_visit: &Value,
) -> Result<Value, String> {
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    let previous_visit = previous_visit
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or("Missing previous composer visit")?;
    let args = json!({"target":label(target),"expectedScope":scope,"previousVisit":previous_visit});
    let mut last = Value::Null;
    for _ in 0..150 {
        ensure(
            crate::view::composer::field_scope(&state.peek(), target) == scope,
            "Protected owner changed while awaiting remount",
        )?;
        // Only explicit mount-pending observations are retried. Geometry and
        // other DOM failures remain fatal, and no projection ticket is captured.
        last = eval(format!("{};try{{dioxus.send(await protectedDraftsDOM(\"ready\",{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
            include_str!("check_dom.js"), args)).await?;
        ensure(
            crate::view::composer::field_scope(&state.peek(), target) == scope,
            "Protected owner changed during remount observation",
        )?;
        if last["ready"] == true {
            return Ok(last);
        }
        ensure(
            last["ready"] == false,
            "Invalid composer readiness observation",
        )?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Protected composer did not remount: {last}"))
}
pub(super) fn same_native(a: &Value, b: &Value) -> bool {
    [
        "text",
        "display",
        "codePoints",
        "utf16Length",
        "scope",
        "visit",
        "start",
        "end",
        "direction",
    ]
    .iter()
    .all(|key| a[*key] == b[*key])
}

pub(super) async fn controls(target: ComposerTarget, click: bool) -> Result<Value, String> {
    eval(format!(r#"
const field=document.getElementById({}),footer=field?.closest('footer');
const button=footer?.querySelector('.composer-row > .composer-send-btn');
const pane=footer?.closest('.thread-pane,.conversation'),header=pane?.querySelector(':scope > header');
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const rect=e=>{{if(!e)return null;const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}};}};
const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1&&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
const sample=(e,r,rounded=false)=>{{
  const proof={{ok:false,method:rounded?'rounded-arc':'rect-grid',points:[]}};
  if(!e||!r||r.width<=0||r.height<=0)return proof;
  let points;
  if(rounded){{
    const style=getComputedStyle(e),names=['borderTopLeftRadius','borderTopRightRadius','borderBottomRightRadius','borderBottomLeftRadius'];
    proof.computedRadii=names.map(name=>style[name]);proof.inset=1;
    const radii=proof.computedRadii.map(value=>{{const parts=value.trim().split(/\s+/);return parts.length<=2&&parts.every(part=>/^\d+(?:\.\d+)?px$/.test(part))&&parts.every(part=>parseFloat(part)===parseFloat(parts[0]))?parseFloat(parts[0]):NaN;}});
    const radius=radii[0];
    if(!Number.isFinite(radius)||!radii.every(value=>value===radius)||radius>Math.min(r.width,r.height)/2||Math.min(r.width,r.height)<=2){{proof.error='Unsupported rounded Send hit shape';return proof;}}
    proof.radius=radius;
    // Square-corner samples can lie outside a rounded button. Keep all four
    // edge midpoints and sample each actual 45-degree arc one CSS px inward.
    const d=radius>1?radius-(radius-1)/Math.SQRT2:1;
    points=[['center',r.width/2,r.height/2],['top',r.width/2,1],['right',r.width-1,r.height/2],['bottom',r.width/2,r.height-1],['left',1,r.height/2],['top-left',d,d],['top-right',r.width-d,d],['bottom-right',r.width-d,r.height-d],['bottom-left',d,r.height-d]];
  }}else{{points=[.05,.5,.95].flatMap((x,i)=>[.05,.5,.95].map((y,j)=>[`${{i}}:${{j}}`,r.width*x,r.height*y]));}}
  proof.points=points.map(([name,dx,dy])=>{{const x=r.x+dx,y=r.y+dy,hit=document.elementFromPoint(x,y);return {{name,x,y,owned:!!hit&&e.contains(hit),element:hit?{{tag:hit.tagName,id:hit.id||'',classes:typeof hit.className==='string'?hit.className:hit.className?.baseVal||''}}:null}};}});
  proof.ok=proof.points.every(point=>point.owned);return proof;
}};
const f=rect(field),c=rect(footer),b=rect(button),p=rect(pane),h=rect(header);
const hitSamples={{field:sample(field,f),composer:sample(footer,c),button:sample(button,b,true)}};
const fieldUnobscured=hitSamples.field.ok,composerUnobscured=hitSamples.composer.ok,buttonUnobscured=hitSamples.button.ok;
const targetMatches=!!pane&&({}==='thread'?pane.classList.contains('thread-pane'):pane.classList.contains('conversation'));
const visible=targetMatches&&inside(c,p)&&inside(f,c)&&inside(b,c)&&inside(h,p)&&c.y>=h.y+h.height-1&&fieldUnobscured&&composerUnobscured&&buttonUnobscured;
const ok=!!field&&!!button&&!button.disabled&&visible;
const result={{ok,kind:'composer',target:{},clickRequested:{},clicked:false,field:f,composer:c,button:b,pane:p,header:h,hitSamples,fieldUnobscured,composerUnobscured,buttonUnobscured,targetMatches,disabled:button?.disabled,scope:field?.dataset.draftScope,visit:field?.dataset.projectionVisit,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}};
if({}&&ok){{button.focus({{preventScroll:true}});button.click();result.clicked=true;}}
result.fieldFocused=document.activeElement===field;result.buttonFocused=document.activeElement===button;
dioxus.send(result);await dioxus.recv();
"#, json!(format!("{}-composer",label(target))),json!(label(target)),json!(label(target)),json!(click),json!(click))).await
}
async fn anchor_measure(expected: &Value) -> Result<Value, String> {
    eval(format!(
        "{};dioxus.send({{ok:true,anchor:scheduleHomeAnchor({})}});await dioxus.recv();",
        include_str!("check_home.js"),
        expected
    ))
    .await
    .map(|v| v["anchor"].clone())
}
fn same_timeline(state: Signal<ShellState>, expected: &Value) -> bool {
    let shell = state.peek();
    json!(shell.timeline_scope()) == expected["scope"]
        && json!(shell.channel_rows.len()) == expected["windowEnd"]
        && json!(shell.channel_rows.last().map(|m| &m.id)) == expected["latestId"]
}
pub(super) async fn home(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Value, String> {
    // Real component unmount/remount restores stored text. Never sets DOM text.
    {
        let mut s = state.write();
        s.main_view = MainView::Unreads;
        s.thread_root = None;
    }
    let mut removed = false;
    for _ in 0..100 {
        if dom(target, "absent", None).await.is_ok() {
            removed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(removed, "Composer did not unmount")?;
    use super_platinum_core::domain::PendingScrollTarget;
    let expected = {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT_TS.into());
        s.sync_composer_drafts();
        if s.thread_root.is_some() {
            s.refresh_thread_from_core();
        }
        // Publish the intended virtual window together with the Home mount.
        // A mounted composer alone does not prove that its timeline tail exists.
        let id = s.core.active_channel.clone().ok_or("Channel missing")?;
        s.core.pending_scroll_to = Some((id, PendingScrollTarget::Latest));
        s.stick_to_bottom = true;
        s.reset_timeline_window();
        json!({"scope":s.timeline_scope(),"latestId":s.channel_rows.last().map(|m|&m.id),"windowEnd":s.channel_rows.len()})
    };
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    let mut last = Value::Null;
    for _ in 0..150 {
        ensure(
            same_timeline(state, &expected),
            "Home timeline identity changed while mounting",
        )?;
        let field = dom(target, "read", None)
            .await
            .ok()
            .map(|v| v["native"].clone());
        last = anchor_measure(&expected).await?;
        if let Some(field) = field
            && field["scope"] == scope
            && field["visit"].as_str().is_some_and(|s| !s.is_empty())
            && last["ready"] == true
        {
            let before = last;
            ensure(
                same_timeline(state, &expected),
                "Home timeline changed before navigation",
            )?;
            let landed =
                crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await;
            frames().await?;
            let mut value = dom(target, "read", None).await?["native"].clone();
            let after = anchor_measure(&expected).await?;
            if !landed
                || !same_timeline(state, &expected)
                || after["atBottom"] != true
                || before["inputGeneration"] != after["inputGeneration"]
                || field["scope"] != value["scope"]
                || field["visit"] != value["visit"]
            {
                return Err(format!(
                    "Timeline anchor did not settle: {}",
                    json!({"landed":landed,"before":before,"after":after,"sameTimeline":same_timeline(state,&expected)})
                ));
            }
            value["homeAnchor"] = json!({"landed":landed,"before":before,"after":after});
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Restored composer/timeline mount unavailable: {last}"
    ))
}
