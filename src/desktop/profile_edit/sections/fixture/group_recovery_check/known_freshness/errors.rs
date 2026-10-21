use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    first: u64,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let remembered = remember(desktop).await?;
    let before = source(state, false)?;
    let first_release = recovery::release_observed(
        first,
        Err(Error::Offline("closed known-group offline".into())),
    )?;
    let fallback = super::super::scenario::request(state, "direct", &recovery::ids()).await?;
    let fallback_release = recovery::release_observed(
        fallback,
        Err(Error::Offline("closed known-group fallback offline".into())),
    )?;
    let completed = settled(state, 1).await?;
    editor(desktop, recovery::DRAFT, false).await?;
    let retained = custody(desktop).await?;
    if source(state, false)? != before {
        return Err("Known-group error changed canonical row".into());
    }
    let observed = quiet(state, desktop, recovery::DRAFT, false).await?;
    capture(
        state,
        desktop,
        name,
        "offline-mounted",
        json!({"remembered":remembered,"custody":retained,"directoryRelease":first_release,
            "fallbackRelease":fallback_release,"settled":completed,"quiet":observed,
            "sourceBefore":before,"sourceAfter":source(state,false)?}),
        stages,
        captures,
    )
    .await?;
    let reopened = reopen(state, desktop, recovery::DRAFT, false).await?;
    capture(
        state,
        desktop,
        name,
        "fresh-reopened",
        reopened,
        stages,
        captures,
    )
    .await?;
    let (second, held) = reset_open(state, desktop, recovery::DRAFT, false).await?;
    let remembered = remember(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "reset-held",
        json!({"held":held,"remembered":remembered}),
        stages,
        captures,
    )
    .await?;
    let directory_raw = json!({"ok":true,"results":"malformed"});
    let direct_raw = json!({"ok":true,"subteams":"malformed"});
    let directory_release = recovery::release_observed(second, Ok(directory_raw.clone()))?;
    let fallback = super::super::scenario::request(state, "direct", &recovery::ids()).await?;
    let direct_release = recovery::release_observed(fallback, Ok(direct_raw.clone()))?;
    let completed = settled(state, 2).await?;
    editor(desktop, recovery::DRAFT, false).await?;
    let retained = custody(desktop).await?;
    if source(state, false)? != before {
        return Err("Malformed known response changed original metadata".into());
    }
    let observed = quiet(state, desktop, recovery::DRAFT, false).await?;
    capture(
        state,
        desktop,
        name,
        "malformed-mounted",
        json!({"directoryRaw":directory_raw,"directRaw":direct_raw,
            "directoryRelease":directory_release,"fallbackRelease":direct_release,
            "settled":completed,"custody":retained,"quiet":observed,
            "sourceBefore":before,"sourceAfter":source(state,false)?}),
        stages,
        captures,
    )
    .await?;
    let reopened = reopen(state, desktop, recovery::DRAFT, false).await?;
    capture(
        state,
        desktop,
        name,
        "final-reopened",
        reopened,
        stages,
        captures,
    )
    .await
}
