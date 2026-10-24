//! Wait for the owned controller's prelude and one real native middle Enter.
use super::*;

pub(super) async fn completed(
    desktop: &dioxus::desktop::DesktopContext,
    marker: &str,
) -> Result<Value, String> {
    let expected = json!([fixture::parent_text(), fixture::child_text()]);
    let count = fixture::pre_enter::updates().len() + 1 + fixture::typing_reports().len();
    let proof=wait(desktop,&format!(r#"(()=>{{
      const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');
      const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit);
      const s=witness?.sequence,events=s?.events;
      return {{ok:witness?.kind==='middle-sequence'&&f.dataset.canvasPreMiddleComplete==={}
        &&events?.length==={}&&events.filter(e=>e.kind==='middle').length===1
        &&JSON.stringify(s.after[0].elements.map(e=>e.elements.map(r=>r.text).join('')))===JSON.stringify({}),witness}};
    }})()"#,json!(marker),count,expected)).await?;
    Ok(proof["witness"].clone())
}
