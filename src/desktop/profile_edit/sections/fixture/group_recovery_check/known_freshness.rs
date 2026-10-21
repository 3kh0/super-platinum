//! A usable known description mounts before its account-owned background read.
//! These checks deliberately do not reuse MissingOpening/loading expectations.
use super::*;
use recovery::known_freshness as known;
mod errors;
mod helpers;
mod retirement;
use helpers::{
    custody, edit, editor, quiet, remember, reopen, reset_open, scheduler, settled, source,
};

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
    recovery::seed(state).await?;
    let original = recovery::snapshot(&state.peek());
    source(state, false)?;
    if !recovery::requests().is_empty()
        || scheduler(state)["freshIds"] != json!(recovery::ids())
        || scheduler(state)["started"] != 0
    {
        return Err(format!(
            "Real group source sink did not establish initial freshness: {original}"
        ));
    }
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"originalDraft":true,"actualSourceSink":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", true).await?;
    editor(desktop, recovery::DRAFT, false).await?;
    let observed = quiet(state, desktop, recovery::DRAFT, false).await?;
    if !recovery::requests().is_empty() {
        return Err("Genuinely fresh known opening issued a read".into());
    }
    capture(
        state,
        desktop,
        name,
        "fresh-open",
        json!({"noRead":true,"quiet":observed}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let (first, held) = reset_open(state, desktop, recovery::DRAFT, false).await?;
    capture(
        state,
        desktop,
        name,
        "mounted-held",
        held,
        &mut stages,
        &mut captures,
    )
    .await?;
    match recovery::case().as_str() {
        "known-freshness" => {
            renamed(state, desktop, name, first, &mut stages, &mut captures).await?
        }
        "known-freshness-errors" => {
            errors::run(state, desktop, name, first, &mut stages, &mut captures).await?
        }
        "known-freshness-retirement" => {
            retirement::run(state, desktop, name, first, &mut stages, &mut captures).await?
        }
        _ => return Err("Unknown known-group scenario".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":recovery::case(),"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}

async fn renamed(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    first: u64,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let edited = edit(desktop).await?;
    editor(desktop, recovery::EDITED, false).await?;
    let remembered = remember(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "edited-held",
        json!({"nativeEdit":edited,"remembered":remembered}),
        stages,
        captures,
    )
    .await?;
    let reply = known::response(true, false);
    let release = recovery::release_observed(first, Ok(reply.clone()))?;
    let completed = settled(state, 1).await?;
    let source = source(state, true)?;
    editor(desktop, recovery::EDITED, false).await?;
    let retained = custody(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "refreshed-mounted",
        json!({"response":reply,"release":release,"settled":completed,"source":source,
            "custody":retained,"mountedOldLabels":true}),
        stages,
        captures,
    )
    .await?;
    click(desktop, "Use description", false).await?;
    form(desktop).await?;
    if mentions::snapshot(&state.peek())["blocks"] != json!(recovery::blocks(recovery::EDITED)) {
        return Err("Known-group Use description changed native raw token bytes".into());
    }
    super::scenario::save_text(state, desktop, recovery::EDITED).await?;
    capture(
        state,
        desktop,
        name,
        "saved",
        json!({"actualProfileSaveReadback":true,"rawDuplicatesPreserved":true}),
        stages,
        captures,
    )
    .await?;
    let count = recovery::requests();
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::EDITED, true).await?;
    let observed = quiet(state, desktop, recovery::EDITED, true).await?;
    if recovery::requests() != count {
        return Err("Renamed current opening repeated a fresh read".into());
    }
    capture(
        state,
        desktop,
        name,
        "reopened",
        json!({"newLabels":true,"noRedundantRead":true,"quiet":observed}),
        stages,
        captures,
    )
    .await?;
    let (second, held) = reset_open(state, desktop, recovery::EDITED, true).await?;
    let remembered = remember(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "equal-held",
        json!({"held":held,"remembered":remembered}),
        stages,
        captures,
    )
    .await?;
    let original = helpers::source(state, true)?;
    let reply = known::response(true, true);
    let release = recovery::release_observed(second, Ok(reply.clone()))?;
    let completed = settled(state, 2).await?;
    if helpers::source(state, true)? != original {
        return Err("Equal positive group version overwrote original canonical metadata".into());
    }
    editor(desktop, recovery::EDITED, true).await?;
    let retained = custody(desktop).await?;
    capture(state,desktop,name,"equal-mounted",
        json!({"response":reply,"release":release,"settled":completed,"custody":retained,
            "sourceBefore":original,"sourceAfter":helpers::source(state,true)?,"equalVersionSkipped":true}),
        stages,captures).await
}
