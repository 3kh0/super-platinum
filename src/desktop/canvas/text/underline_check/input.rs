use super::*;
const INPUT: &str = include_str!("input.js");
pub(super) async fn edit_and_toggle(
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    checked(
        desktop,
        &format!("(()=>{{{INPUT}\nreturn underlineAction('insert');}})()"),
    )
    .await?;
    for expected in [true, false, true] {
        checked(
            desktop,
            &format!("(()=>{{{INPUT}\nreturn underlineAction('toggle');}})()"),
        )
        .await?;
        // Poll only the read; recording the proof is once-only after readiness.
        wait(desktop,&format!(r#"(()=>{{const b=window.superPlatinumRichEditor.read('canvas-text-draft')[0].elements[0].elements;return {{ok:b.filter(r=>r.text?.includes('toggle')).some(r=>r.style?.underline===true)==={expected}}};}})()"#)).await?;
        checked(
            desktop,
            &format!(
                "(()=>{{{INPUT}\nreturn underlineAction('{}');}})()",
                if expected { "record-on" } else { "record-off" }
            ),
        )
        .await?;
    }
    Ok(())
}
