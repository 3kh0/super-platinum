//! Target the new pre-split history preview within the existing recovery capture.
use super::*;
pub(super) async fn reveal(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    checked(desktop,r#"(()=>{const n=document.querySelector('[data-review-pre-enter]');if(!n)return {ok:false};n.scrollIntoView({block:'center',behavior:'instant'});return {ok:true};})()"#).await?;
    let expected = format!(
        "{}{}",
        super::super::split_fixture::pre_enter::parent_text(),
        super::super::split_fixture::corpus().right
    );
    wait(desktop,&format!(r#"(()=>{{
      const n=document.querySelector('[data-review-pre-enter]'),s=n?.closest('.canvas-scroll');
      const r=n?.getBoundingClientRect(),b=s?.getBoundingClientRect();
      const text=n?.querySelector('.canvas-review-preview')?.textContent;
      const notice=n?.querySelector('p')?.textContent;
      const visible=!!r&&!!b&&r.width>0&&r.height>0&&r.left>=Math.max(0,b.left)-1&&r.right<=Math.min(innerWidth,b.right)+1&&r.top>=Math.max(0,b.top)-1&&r.bottom<=Math.min(innerHeight,b.bottom)+1;
      return {{ok:visible&&text==={}&&notice==='Edits made before Enter remain part of this unresolved change, even when the text was changed back.',preEnter:true,text,historyNotice:notice,visible,rect:r?[r.x,r.y,r.width,r.height]:null,viewport:[innerWidth,innerHeight]}};
    }})()"#,json!(expected))).await
}
