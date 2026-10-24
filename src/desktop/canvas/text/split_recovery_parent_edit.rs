//! Reveal the initial first-paragraph preview retained beside its later draft.
use super::*;
pub(super) async fn reveal(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    checked(desktop,r#"(()=>{const n=document.querySelector('[data-review-parent-history]');if(!n)return {ok:false};n.scrollIntoView({block:'center',behavior:'instant'});return {ok:true};})()"#).await?;
    wait(desktop,&format!(r#"(()=>{{
      const n=document.querySelector('[data-review-parent-history]'),s=n?.closest('.canvas-scroll');
      const r=n?.getBoundingClientRect(),b=s?.getBoundingClientRect();
      const text=n?.querySelector('.canvas-review-preview')?.textContent,notice=n?.querySelector('p')?.textContent;
      const visible=!!r&&!!b&&r.width>0&&r.height>0&&r.left>=Math.max(0,b.left)-1&&r.right<=Math.min(innerWidth,b.right)+1&&r.top>=Math.max(0,b.top)-1&&r.bottom<=Math.min(innerHeight,b.bottom)+1;
      return {{ok:visible&&text==={}&&notice==='The first paragraph was edited after Enter. Those edits remain part of this unresolved change, even when the text was changed back.',parentHistory:true,text,historyNotice:notice,visible,rect:r?[r.x,r.y,r.width,r.height]:null,viewport:[innerWidth,innerHeight]}};
    }})()"#,json!(super::super::split_fixture::parent_edit::initial_text()))).await
}
