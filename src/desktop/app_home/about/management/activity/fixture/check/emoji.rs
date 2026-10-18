//! Native observations for the bounded Activity message emoji cases.
use super::*;

pub(super) async fn inspect(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop, SCRIPT).await
}

const SCRIPT: &str = r#"(()=>{
 const trigger=document.getElementById('workflow-activity-trigger-message'),step=document.getElementById('workflow-activity-step-message-0');
 if(!trigger||!step)return{ok:false};
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const r=rect(e),s=getComputedStyle(e);if(r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),c=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(c.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(c.overflowX)&&(r.left<q.left-1||r.right>q.right+1))return false}return true};
 const paragraphs=[trigger,step].map(e=>{e.scrollIntoView({block:'nearest'});const spans=[...e.querySelectorAll('span')].map(n=>{const s=getComputedStyle(n);return{display:s.display,margins:[s.marginTop,s.marginRight,s.marginBottom,s.marginLeft]}});return{id:e.id,text:e.textContent,visible:visible(e),rect:rect(e),scrollSample:true,spans}});
 const images=[...trigger.querySelectorAll('img')].map(e=>({alt:e.alt,src:e.getAttribute('src'),complete:e.complete,naturalWidth:e.naturalWidth,naturalHeight:e.naturalHeight}));
 const coldFallback=trigger.querySelector('[title=":activity_cold:"]')?.textContent??null;
 const coldImages=trigger.querySelectorAll('img[alt=":activity_cold:"]').length;
 const waves=e=>[...e.querySelectorAll('.custom-emoji')].filter(n=>n.textContent==='\u{1f44b}');
 const explicit=waves(trigger),trailing=waves(step),standardGlyphs=[...explicit,...trailing].map(e=>e.textContent);
 const explicitTitle=explicit[0]?.getAttribute('title')??null,trailingTitle=trailing[0]?.getAttribute('title')??null;
 const forbiddenNodes=[trigger,step].reduce((n,e)=>n+e.querySelectorAll('a,button,b,i,script,iframe').length,0);
 const paragraphCount=document.querySelectorAll('#workflow-activity-detail .workflow-activity-message').length;
 const inlineMargins=paragraphs.every(p=>p.spans.length>0&&p.spans.every(s=>['inline','inline-block'].includes(s.display)&&s.margins.every(v=>parseFloat(v)===0)));
 const expectedTrigger='Prefix :wave: \u{1f44b}  :activity_cold:';
 const expectedStep='L<@U1><b>x</b>replacement.value{no}{zero} \u4e2d\u{1f642} <@U1><i>y</i> \u{1f44b}';
 return{ok:paragraphCount===2&&paragraphs.every(p=>p.visible)&&paragraphs[0].text===expectedTrigger&&paragraphs[1].text===expectedStep&&images.length===1&&images[0].alt===':activity_ready:'&&images[0].complete&&images[0].naturalWidth===16&&images[0].naturalHeight===16&&coldFallback===':activity_cold:'&&coldImages===0&&explicit.length===1&&trailing.length===1&&explicitTitle===null&&trailingTitle===':wave:'&&JSON.stringify(standardGlyphs)===JSON.stringify(['\u{1f44b}','\u{1f44b}'])&&forbiddenNodes===0&&inlineMargins,
 viewport:[innerWidth,innerHeight],paragraphCount,paragraphs,images,coldFallback,coldImages,standardGlyphs,explicitGlyphCount:explicit.length,trailingGlyphCount:trailing.length,explicitTitle,trailingTitle,forbiddenNodes,inlineMargins,measurement:'individual-scroll-samples'};
})()"#;
