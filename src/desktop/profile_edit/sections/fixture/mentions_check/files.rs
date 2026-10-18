//! Existing whole-document tokens, through the actual cold read and editor.
use super::super::files as source;
use super::*;
use crate::profile_edit::sections::rich::files as cache;

/// These are computed DOM styles from the same geometry sample as the capture,
/// not a restatement of the raw token's style map.
pub(super) fn styles(geometry: &Value) -> Result<(), String> {
    let tokens = geometry["tokens"]
        .as_array()
        .ok_or("Missing file token styles")?;
    if geometry["richEditable"].is_null() && tokens.is_empty() {
        return Ok(());
    }
    if tokens.len() != 2
        || tokens[0]["fontWeight"] != "700"
        || tokens[0]["fontStyle"] != "normal"
        || tokens[1]["fontWeight"] != "400"
        || tokens[1]["fontStyle"] != "italic"
    {
        return Err(format!(
            "File token computed formatting differs: {tokens:?}"
        ));
    }
    Ok(())
}

pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"error":error,"source":source::status(),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"File titles timed out","source":source::status()})
            }
        };
        eprintln!("profile-file-titles-check: {result}");
    });
}
async fn requests(count: usize, completed: usize, held: bool) -> Result<(), String> {
    for _ in 0..100 {
        let s = source::status();
        if s["requests"].as_array().map(Vec::len) == Some(count)
            && s["completed"] == completed
            && s["held"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("File request settlement: {}", source::status()))
}
fn invalidate(mut state: Signal<ShellState>) -> Result<(), String> {
    let mut shell = state.write();
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    cache::invalidate(&mut shell, &team, source::ID);
    Ok(())
}
async fn message(desktop: &DesktopContext, loading: bool) -> Result<(), String> {
    let text = if loading {
        "Loading document titles. Your description is unchanged."
    } else {
        "Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details."
    };
    wait_dom(desktop,&format!("({{ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==={}}})",json!(text))).await.map(|_|())
}
async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    extra: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let observed = cache::fixture_status(&state.peek(), source::ID);
    if source::block_mode() && stage != "cold-loading" {
        let expected = source::title_blocks(matches!(stage, "renamed-mounted" | "confirmed"));
        if observed["source"]["title_blocks"] != expected
            || observed["effective"]["title_blocks"] != expected
            || observed["quipTitle"]
                != json!({"current":true,"value":"nullish","fileId":source::ID})
        {
            return Err(format!(
                "Missing original block title or actual runtime absence observation: {observed}"
            ));
        }
    }
    let proof = json!({"source":source::status(),"cache":observed,"exercise":extra});
    capture(state, desktop, name, stage, proof, stages, captures).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::enabled(&state.peek()) {
        return Err("Closed file identity missing".into());
    }
    select_values(state, desktop).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    if !cache::fixture_status(&state.peek(), source::ID).is_null() {
        return Err("Cold file cache was preseeded".into());
    }
    source::mode(source::Mode::Held);
    click(desktop, "Edit description", false).await?;
    requests(1, 0, true).await?;
    message(desktop, true).await?;
    shot(
        state,
        desktop,
        name,
        "cold-loading",
        json!({"cold":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let released = source::release();
    if released["delivered"] != true {
        return Err("Cold response not delivered".into());
    }
    requests(1, 1, false).await?;
    editor_label(desktop, "Working with ", true, source::mounted_label()).await?;
    shot(
        state,
        desktop,
        name,
        "opened",
        json!({"release":released}),
        &mut stages,
        &mut captures,
    )
    .await?;
    cache::fixture_observe(&mut state.write(), &source::raw(true)).map_err(|e| e.to_string())?;
    editor_label(desktop, "Working with ", true, source::mounted_label()).await?;
    shot(
        state,
        desktop,
        name,
        "renamed-mounted",
        json!({"frozenLabel":source::mounted_label()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    edit_label(desktop, "Saved with ", source::mounted_label()).await?;
    use_description(state, desktop, "Saved with ").await?;
    save(state, desktop, Outcome::Saved, 1, 1).await?;
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Saved with ", true, source::renamed_label()).await?;
    shot(
        state,
        desktop,
        name,
        "confirmed",
        json!({"reopenedLabel":source::renamed_label()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    cache::fixture_observe(&mut state.write(), &source::raw(false)).map_err(|e| e.to_string())?;
    invalidate(state)?;
    source::mode(source::Mode::Offline);
    click(desktop, "Edit description", false).await?;
    requests(2, 2, false).await?;
    message(desktop, false).await?;
    shot(
        state,
        desktop,
        name,
        "unavailable",
        json!({"draftRetained":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    source::mode(source::Mode::Normal);
    click(desktop, "Retry description", false).await?;
    requests(3, 3, false).await?;
    editor_label(desktop, "Saved with ", true, source::mounted_label()).await?;
    shot(
        state,
        desktop,
        name,
        "retried",
        json!({"draftRetained":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    invalidate(state)?;
    source::mode(source::Mode::Held);
    click(desktop, "Edit description", false).await?;
    requests(4, 3, true).await?;
    message(desktop, true).await?;
    shot(
        state,
        desktop,
        name,
        "held",
        json!({"originalOpening":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;
    requests(4, 4, true).await?;
    let released = source::release();
    if released != json!({"delivered":false,"cancelledBeforeReply":true})
        || cache::fixture_status(&state.peek(), source::ID)["current"] != false
    {
        return Err("Canceled opening received or installed late file response".into());
    }
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await?;
    shot(
        state,
        desktop,
        name,
        "cancelled",
        json!({"release":released}),
        &mut stages,
        &mut captures,
    )
    .await?;
    source::mode(source::Mode::Normal);
    click(desktop, "Edit description", false).await?;
    requests(5, 5, false).await?;
    editor_label(desktop, "Saved with ", true, source::mounted_label()).await?;
    let label_length = source::mounted_label().encode_utf16().count();
    let limit = "x".repeat(5000 - 2 * label_length);
    edit_label(desktop, &limit, source::mounted_label()).await?;
    use_description(state, desktop, &limit).await?;
    if mentions::snapshot(&state.peek())["canSave"] != true {
        return Err("5000 label units refused".into());
    }
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, &limit, true, source::mounted_label()).await?;
    shot(
        state,
        desktop,
        name,
        "limit5000",
        json!({"sourceLength":5000,"labelUnits":label_length,"occurrences":2}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let excess = format!("{limit}x");
    edit_label(desktop, &excess, source::mounted_label()).await?;
    use_description(state, desktop, &excess).await?;
    if mentions::snapshot(&state.peek())["canSave"] != false {
        return Err("5001 label units accepted".into());
    }
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, &excess, true, source::mounted_label()).await?;
    click(desktop, "Cancel description edit", false).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.profile-edit-field-error')?.textContent==="Check this field's value or length."})"#).await?;
    shot(
        state,
        desktop,
        name,
        "limit5001",
        json!({"sourceLength":5001,"labelUnits":label_length,"occurrences":2}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"readiness":ready,"mentionKind":source::kind(),"titleBlocks":source::block_mode(),"stages":stages,
        "captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,
        "visualReview":"Original PNG inspection required separately"}),
    )
}
