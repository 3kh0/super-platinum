//! Both members remain ordinarily editable after a typed middle split.
use super::*;

pub(super) async fn continue_pair(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let proof = fixture::proof(&state.peek());
    let child = proof["writes"][0]["child"]
        .as_str()
        .ok_or("Missing split child")?
        .to_owned();
    for (target, initial, replacement, ordinal, writes, names) in [
        (
            child.as_str(),
            fixture::child_text(),
            fixture::continued_text(),
            4,
            2,
            if fixture::is_start() {
                ["new-editing", "new-saving", "new-confirmed"]
            } else {
                ["child-editing", "continued-saving", "continued-confirmed"]
            },
        ),
        (
            fixture::parent_id(),
            fixture::parent_text(),
            fixture::corpus().edited,
            7,
            3,
            if fixture::is_start() {
                ["original-editing", "original-saving", "original-confirmed"]
            } else {
                ["parent-editing", "parent-saving", "parent-confirmed"]
            },
        ),
    ] {
        click(
            desktop,
            &format!(
                "[data-canvas-text-section={}] .canvas-text-open",
                json!(target)
            ),
        )
        .await?;
        wait(desktop,&format!(r#"(()=>{{const f=document.getElementById('canvas-text-draft');return {{ok:f?.isContentEditable&&f.textContent==={}}};}})()"#,json!(initial))).await?;
        stage(state, desktop, names[0], ordinal, stages).await?;
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
        stage(state, desktop, names[1], ordinal + 1, stages).await?;
        if !fixture::complete_text(state).await {
            return Err(format!(
                "{target} continuation ACK/readback failed: {}",
                fixture::proof(&state.peek())["acknowledgements"]
            ));
        }
        wait(desktop,&format!(r#"(()=>{{const p=document.querySelector('[data-canvas-text-section="'+{}+'"] .canvas-paragraph-content'),c=document.querySelector('[data-canvas-text-section="'+{}+'"] .canvas-paragraph-content');return {{ok:!document.getElementById('canvas-text-editor')&&p?.textContent==={}&&c?.textContent==={}}};}})()"#,json!(fixture::parent_id()),json!(child),json!(if target==fixture::parent_id(){replacement}else{fixture::parent_text()}),json!(fixture::continued_text()))).await?;
        stage(state, desktop, names[2], ordinal + 2, stages).await?;
    }
    Ok(())
}
