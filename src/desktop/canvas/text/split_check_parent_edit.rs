//! Verify completion of the owned controller's middle split and row edits.
use super::*;
pub(super) async fn completed(
    desktop: &dioxus::desktop::DesktopContext,
    marker: &str,
) -> Result<Value, String> {
    let expected = json!([fixture::parent_text(), fixture::child_text()]);
    let count = fixture::pre_enter::updates().len()
        + 1
        + fixture::parent_edit::updates().len()
        + fixture::typing_reports().len();
    let proof=wait(desktop,&format!(r#"(()=>{{
      const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');
      const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit);
      const s=witness?.sequence,events=s?.events;
      return {{ok:witness?.kind==='middle-sequence'&&f.dataset.canvasMiddleParentComplete==={}
        &&events?.length==={}&&events.filter(e=>e.kind==='middle').length===1
        &&events.slice(events.findIndex(e=>e.kind==='middle')+1).some(e=>e.kind==='edit'&&e.activeIndex===0)
        &&JSON.stringify(s.after[0].elements.map(e=>e.elements.map(r=>r.text).join('')))===JSON.stringify({}),witness}};
    }})()"#,json!(marker),count,expected)).await?;
    Ok(proof["witness"].clone())
}
