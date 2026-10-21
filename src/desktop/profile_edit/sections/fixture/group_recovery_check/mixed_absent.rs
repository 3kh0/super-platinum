//! File, genuinely absent member, then group admission through original readers.
use super::super::{files, member_recovery as member};
use super::mixed_files::{file_request, message, ready_file};
use super::scenario::{directory, finished, request, retained, save_text, settled};
use super::*;
mod evidence;
use evidence::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
    recovery::seed(state).await?;
    let original = recovery::snapshot(&state.peek());
    absent(state)?;
    if !files::status()["requests"]
        .as_array()
        .is_some_and(Vec::is_empty)
        || !member::requests().is_empty()
        || !recovery::requests().is_empty()
        || !original["mixedFile"]["cache"].is_null()
    {
        return Err(format!(
            "Mixed absence was not original and cold: {original}"
        ));
    }
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    shot(
        state,
        desktop,
        name,
        "initial",
        json!({"targetTrulyAbsent":true,"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    files::mode(files::Mode::Held);
    click(desktop, "Edit description", true).await?;
    file_request(1, 0, true).await?;
    message(desktop, true).await?;
    absent(state)?;
    if !member::requests().is_empty() || !recovery::requests().is_empty() {
        return Err("Mixed later readers started before original file response".into());
    }
    let first = opening(state);
    quiet(state, desktop, Some(true)).await?;
    shot(
        state,
        desktop,
        name,
        "file-held",
        json!({"duplicateOpen":true,"fileFirst":true,"opening":first,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let file_release = release_file()?;
    file_request(1, 1, false).await?;
    let mut pending = member_request(state).await?;
    ready_file(state)?;
    absent(state)?;
    no_groups(state)?;
    same_opening(state, &first)?;
    quiet(state, desktop, Some(true)).await?;
    shot(
        state,
        desktop,
        name,
        "member-held",
        json!({"fileRelease":file_release,"fileReady":true,"originalOpening":first,
            "coalescedMemberIds":true,"noPrematureGroups":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    if recovery::case() == "mixed-absent-member-offline" {
        let before = member_finished(state);
        let released = member::release_observed(
            pending,
            Err(Error::Offline("closed mixed absent member offline".into())),
        )?;
        require_delivery(&released)?;
        member_settled(state, before).await?;
        absent(state)?;
        no_groups(state)?;
        ready_file(state)?;
        message(desktop, false).await?;
        quiet(state, desktop, Some(false)).await?;
        shot(
            state,
            desktop,
            name,
            "member-offline",
            json!({"memberRelease":released,"noPlaceholder":true,"noAutomaticLoop":true,
                "quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let prior_member = member::requests();
        click(desktop, "Retry description", true).await?;
        file_request(2, 1, true).await?;
        message(desktop, true).await?;
        if member::requests() != prior_member {
            return Err(
                "Explicit mixed Retry started member read before forced file response".into(),
            );
        }
        no_groups(state)?;
        quiet(state, desktop, Some(true)).await?;
        shot(
            state,
            desktop,
            name,
            "retry-file-held",
            json!({"explicitRetry":true,"duplicateRetry":true,"fileFirst":true,
                "quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let released = release_file()?;
        file_request(2, 2, false).await?;
        pending = member_request(state).await?;
        absent(state)?;
        no_groups(state)?;
        ready_file(state)?;
        quiet(state, desktop, Some(true)).await?;
        shot(
            state,
            desktop,
            name,
            "retry-member-held",
            json!({"fileRelease":released,"fileReady":true,"noPrematureGroups":true,
                "quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }

    let member_raw = member::success(state.peek().core.active_team.as_deref().unwrap());
    if recovery::case() == "mixed-absent-member-cancel" {
        click(desktop, "Cancel description edit", false).await?;
        form(desktop).await?;
        retained(state)?;
        no_groups(state)?;
        quiet(state, desktop, None).await?;
        shot(
            state,
            desktop,
            name,
            "cancelled",
            json!({"cancelledOriginalConsumer":true,"noPrematureGroups":true,
                "quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = member_finished(state);
        let released = member::release_observed(pending, Ok((member_raw.clone(), None)))?;
        require_delivery(&released)?;
        member_settled(state, before).await?;
        // A root-owned network outcome is not private renderer adoption.
        // Do not fabricate global source absence/publication from task completion.
        form(desktop).await?;
        retained(state)?;
        no_groups(state)?;
        quiet(state, desktop, None).await?;
        shot(
            state,
            desktop,
            name,
            "late-settled",
            json!({"memberRelease":released,"memberResponse":member_raw,
                "sharedOutcomeSettled":true,"oldConsumerDidNotMount":true,
                "noPrematureGroups":true,"quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let member_before = member::requests();
        let file_before = files::status();
        click(desktop, "Edit description", true).await?;
        let group = request(state, "directory", &recovery::ids()).await?;
        if member::requests() != member_before || files::status() != file_before {
            return Err("Fresh mixed consumer redundantly fetched retained current sources".into());
        }
        finish(
            state,
            desktop,
            name,
            ready,
            group,
            member_raw,
            json!({"freshConsumerAdopted":true,"noRedundantMemberRead":true}),
            stages,
            captures,
        )
        .await
    } else {
        let before = member_finished(state);
        let released = member::release_observed(pending, Ok((member_raw.clone(), None)))?;
        require_delivery(&released)?;
        member_settled(state, before).await?;
        let group = request(state, "directory", &recovery::ids()).await?;
        finish(
            state,
            desktop,
            name,
            ready,
            group,
            member_raw,
            json!({"memberRelease":released,"actualMemberReadAndAdoption":true}),
            stages,
            captures,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn finish(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
    group: u64,
    member_raw: Value,
    proof: Value,
    mut stages: Vec<Value>,
    mut captures: serde_json::Map<String, Value>,
) -> Result<Value, String> {
    canonical_member(state, &member_raw)?;
    ready_file(state)?;
    retained(state)?;
    quiet(state, desktop, Some(true)).await?;
    shot(
        state,
        desktop,
        name,
        "group-held",
        json!({"member":proof,"memberResponse":member_raw,"memberCanonical":true,
            "fileReady":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let before = finished(state);
    recovery::release(group, Ok(directory(&recovery::ids())))?;
    settled(state, before).await?;
    editor(desktop).await?;
    retained(state)?;
    canonical_member(state, &member_raw)?;
    ready_file(state)?;
    shot(
        state,
        desktop,
        name,
        "success",
        json!({"allOriginalReferencesAdmitted":true,"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let edited = wait_dom(desktop, &format!(r#"(()=>{{
        const e=document.getElementById('profile-section-rich');
        const leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');
        if(!e?.isContentEditable||!leaf)return{{ok:false}};
        const before=window.superPlatinumRichEditor.resolved(e);
        leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));
        return{{ok:true,before,after:window.superPlatinumRichEditor.resolved(e)}};
    }})()"#, json!(recovery::EDITED))).await?;
    editor_text(desktop, recovery::EDITED).await?;
    retained(state)?;
    shot(
        state,
        desktop,
        name,
        "edited",
        json!({"actualNativeEdit":true,"nativeEdit":edited}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Use description", false).await?;
    form(desktop).await?;
    if mentions::snapshot(&state.peek())["blocks"] != json!(recovery::blocks(recovery::EDITED)) {
        return Err("Mixed Use changed original rich tokens".into());
    }
    let readers = json!([files::status(), member::requests(), recovery::requests()]);
    save_text(state, desktop, recovery::EDITED).await?;
    shot(
        state,
        desktop,
        name,
        "saved",
        json!({"actualProfileSaveReadback":true,"rawDuplicatesPreserved":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    editor_text(desktop, recovery::EDITED).await?;
    canonical_member(state, &member_raw)?;
    if readers != json!([files::status(), member::requests(), recovery::requests()]) {
        return Err("Canonical mixed reopening repeated original metadata reads".into());
    }
    shot(
        state,
        desktop,
        name,
        "reopened",
        json!({"noRedundantRead":true,"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":recovery::case(),"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "requests":recovery::requests(),"memberRequests":member::requests(),
        "captureChecksPassed":true,"visualReview":"required"}),
    )
}
