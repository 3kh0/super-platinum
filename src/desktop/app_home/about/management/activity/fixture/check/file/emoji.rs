//! Original ready-image mutation, native refusal, and same-node restoration.
use super::*;

pub(super) async fn refusal(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    native::reveal_control(d, FIRST_LINK).await?;
    let before = snapshot(&s.peek());
    let files = endpoint::snapshot(&s.peek());
    let target = destination::snapshot(&s.peek());
    let attempt = wait_dom(d, r#"(()=>{
 const anchor=document.getElementById('workflow-activity-file-trigger-1'),
 name=anchor?.querySelector('.workflow-activity-file-name'),
 image=name?.querySelector('img'),native=document.getElementById('workflow-management')?.__workflowManagement;
 if(!anchor?.isConnected||!name||!image?.complete||image.naturalWidth!==16||image.alt!==':activity_ready_alias:'||!native)return{ok:false};
 const children=[...name.childNodes],alt=image.getAttribute('alt'),src=image.getAttribute('src'),refusals=native.channelRefusals||0;
 window.__activityFileEmojiOriginal={anchor,name,image,children,alt,src};
 image.setAttribute('alt','forged Activity file emoji');
 const proof={ok:true,refusals,originalAlt:alt,mutatedAlt:image.alt,src,semanticLabel:name.dataset.fileLabel,part:anchor.dataset.part,originalConnected:true};
 anchor.click();return proof;
})()"#).await?;
    let observed = wait_dom(d, &format!(r#"(()=>{{
 const p=window.__activityFileEmojiOriginal,n=document.getElementById('workflow-management')?.__workflowManagement,
 notice=document.getElementById('workflow-activity-navigation-notice');
 const originalAnchor=!!p&&document.getElementById('workflow-activity-file-trigger-1')===p.anchor,
 originalName=!!p&&p.anchor.querySelector('.workflow-activity-file-name')===p.name,
 originalImage=!!p&&p.name.querySelector('img')===p.image;
 return{{ok:originalAnchor&&originalName&&originalImage&&p.image.isConnected&&p.image.alt==='forged Activity file emoji'&&
 n?.channelRefusals==={}&&notice?.textContent==='Finish editing the current text before opening this List. Your draft is unchanged.',
 refusals:n?.channelRefusals,notice:notice?.textContent,alt:p?.image.alt,originalImage,originalName,originalAnchor}};
}})()"#,attempt["refusals"].as_u64().ok_or("Missing original refusal counter")?+1)).await?;
    if snapshot(&s.peek()) != before
        || endpoint::snapshot(&s.peek()) != files
        || destination::snapshot(&s.peek()) != target
    {
        return Err("Changed title image dispatched or changed original workflow".into());
    }
    let restored = wait_dom(d, r#"(()=>{
 const p=window.__activityFileEmojiOriginal;if(!p)return{ok:false};
 const originalAnchor=document.getElementById('workflow-activity-file-trigger-1')===p.anchor,
 originalName=p.anchor.querySelector('.workflow-activity-file-name')===p.name,
 originalImage=p.name.querySelector('img')===p.image,
 originalChildren=p.children.length===p.name.childNodes.length&&p.children.every((n,i)=>n===p.name.childNodes[i]);
 if(!originalAnchor||!originalName||!originalImage||!originalChildren||!p.image.isConnected||p.image.getAttribute('src')!==p.src)return{ok:false};
 p.image.setAttribute('alt',p.alt);
 return{ok:true,alt:p.image.alt,src:p.image.getAttribute('src'),originalImage,originalName,originalAnchor,originalChildren};
})()"#).await?;
    labels(d, false).await?;
    if snapshot(&s.peek()) != before
        || endpoint::snapshot(&s.peek()) != files
        || destination::snapshot(&s.peek()) != target
    {
        return Err("Same-node title restoration changed original custody".into());
    }
    Ok(
        json!({"kind":"title-image-alt","before":before,"filesBefore":files,
        "destinationBefore":target,"attempt":attempt,"native":observed,"restored":restored,
        "unchangedRustState":true,"noFileRead":true,"noDestination":true}),
    )
}
