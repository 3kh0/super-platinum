//! Actual two-Enter capture and independently reopened creation members.
use super::*;

pub(super) async fn completed(
    desktop: &dioxus::desktop::DesktopContext,
    marker: &str,
) -> Result<Value, String> {
    let expected = fixture::second_middle::visual_order().map(fixture::second_middle::final_text);
    let proof = wait(desktop, &format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft'),r=document.getElementById('canvas-text-editor');const witness=window.superPlatinumCanvasSplit.capture(f,r.dataset.editorGeneration,r.dataset.canvasVisit),s=witness?.sequence;return {{ok:witness?.kind==='second-middle-sequence'&&f.dataset.canvasSecondMiddleComplete==={}&&s?.events.filter(e=>e.kind==='middle').length===1&&s.events.filter(e=>e.kind==='second-middle').length===1&&JSON.stringify(s.after[0].elements.map(s=>s.elements.map(r=>r.text).join('')))===JSON.stringify({}),witness}};}})()"#, json!(marker), json!(expected))).await?;
    Ok(proof["witness"].clone())
}

pub(super) async fn continue_members(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let proof = fixture::proof(&state.peek());
    let members = proof["writes"][0]["members"]
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or("Missing second-split members")?;
    let mut steps = Vec::new();
    for (i, member) in members.iter().enumerate() {
        steps.push((
            member["id"]
                .as_str()
                .ok_or("Missing second-split identity")?
                .to_owned(),
            fixture::second_middle::final_text(i + 1).into(),
            fixture::second_middle::edited(i + 1),
            [
                format!("member-{i}-editing"),
                format!("member-{i}-saving"),
                format!("member-{i}-confirmed"),
            ],
        ));
    }
    steps.push((
        fixture::parent_id().into(),
        fixture::second_middle::final_text(0).into(),
        fixture::second_middle::edited(0),
        [
            "parent-editing".into(),
            "parent-saving".into(),
            "parent-confirmed".into(),
        ],
    ));
    sequence::continue_steps(state, desktop, stages, steps).await
}
