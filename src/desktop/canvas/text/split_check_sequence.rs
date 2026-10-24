//! Every ordered appended member and original remain editable after confirmation.
use super::*;

pub(super) async fn continue_members(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let proof = fixture::proof(&state.peek());
    let members = proof["writes"][0]["members"]
        .as_array()
        .ok_or("Missing sequence members")?;
    let mut steps = Vec::new();
    for (i, member) in members.iter().enumerate().rev() {
        let target = member["id"]
            .as_str()
            .ok_or("Missing sequence member identity")?
            .to_owned();
        steps.push((
            target,
            fixture::sequence::final_text(i),
            fixture::sequence::edited(i),
            [
                format!("member-{i}-editing"),
                format!("member-{i}-saving"),
                format!("member-{i}-confirmed"),
            ],
        ));
    }
    steps.push((
        fixture::parent_id().into(),
        fixture::sequence::parent_final(),
        fixture::corpus().edited.into(),
        [
            "parent-editing".into(),
            "parent-saving".into(),
            "parent-confirmed".into(),
        ],
    ));
    continue_steps(state, desktop, stages, steps).await
}

pub(super) async fn continue_steps(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: &mut Vec<Value>,
    steps: Vec<(String, String, String, [String; 3])>,
) -> Result<(), String> {
    for (step, (target, initial, replacement, names)) in steps.into_iter().enumerate() {
        let ordinal = 4 + step * 3;
        let writes = step + 2;
        click(
            desktop,
            &format!(
                "[data-canvas-text-section={}] .canvas-text-open",
                json!(target)
            ),
        )
        .await?;
        wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft');return {{ok:f?.isContentEditable&&f.textContent==={}}};}})()"#,json!(initial))).await?;
        stage(state, desktop, &names[0], ordinal, stages).await?;
        let html = format!(
            "<div>{}</div>",
            replacement
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        );
        super::super::check::set_draft(desktop, &html).await?;
        click(desktop, "#canvas-text-save").await?;
        wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let proof = fixture::proof(&state.peek());
            if proof["pending"] == true && proof["writes"].as_array().map(Vec::len) == Some(writes)
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("{target} continuation did not dispatch"));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        stage(state, desktop, &names[1], ordinal + 1, stages).await?;
        if !fixture::complete_text(state).await {
            return Err(format!(
                "{target} continuation ACK/readback failed: {}",
                fixture::proof(&state.peek())["acknowledgements"]
            ));
        }
        wait(desktop,&format!(r#"(()=>{{const row=document.querySelector('[data-canvas-text-section="'+{}+'"] .canvas-paragraph-content');return {{ok:!document.getElementById('canvas-text-editor')&&row?.textContent==={}}};}})()"#,json!(target),json!(replacement))).await?;
        if target == fixture::parent_id() {
            stage(state, desktop, &names[2], ordinal + 2, stages).await?;
        } else {
            stage_for_section(
                state,
                desktop,
                &names[2],
                ordinal + 2,
                stages,
                &target,
                &replacement,
            )
            .await?;
        }
    }
    Ok(())
}
