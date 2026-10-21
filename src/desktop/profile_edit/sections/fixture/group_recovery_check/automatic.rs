//! Automatic absent-only admission, bounded failure and real native editing.
use super::scenario::{
    directory, finished, loading, request, retained, save_text, settled, unchanged,
};
use super::*;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
    let case = recovery::case();
    recovery::seed(state).await?;
    let original = recovery::snapshot(&state.peek());
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    retained(state)?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", true).await?;
    if case != "ready" {
        let first = request(state, "directory", &recovery::ids()).await?;
        loading(desktop).await?;
        quiet(state, desktop, true).await?;
        capture(
            state,
            desktop,
            name,
            "held",
            json!({"automaticRead":true,"duplicateIdsCoalesced":true,"noAutomaticLoop":true,
                "duplicateOpen":true,"quietObservationMs":500}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = finished(state);
        if matches!(case.as_str(), "automatic" | "mixed-member") {
            recovery::release(first, Ok(directory(&recovery::ids())))?;
            settled(state, before).await?;
        } else {
            let failure = match case.as_str() {
                "pending" => Ok(json!({"ok":true,"results":[],"pending_ids":recovery::ids()})),
                "automatic-offline" => {
                    Err(Error::Offline("closed automatic groups offline".into()))
                }
                "automatic-malformed" => Ok(json!({"ok":true,"results":"malformed"})),
                _ => return Err("Unknown automatic group scenario".into()),
            };
            recovery::release(first, failure)?;
            if case != "pending" {
                let fallback = request(state, "direct", &recovery::ids()).await?;
                recovery::release(
                    fallback,
                    if case == "automatic-offline" {
                        Err(Error::Offline("closed automatic fallback offline".into()))
                    } else {
                        Ok(json!({"ok":true,"subteams":"malformed"}))
                    },
                )?;
            }
            settled(state, before).await?;
            unchanged(state, &original)?;
            unavailable(desktop).await?;
            quiet(state, desktop, false).await?;
            let stage = match case.as_str() {
                "pending" => "pending",
                "automatic-offline" => "offline",
                _ => "malformed",
            };
            capture(state, desktop, name, stage,
                json!({"sourceRetained":true,"draftRetained":true,"noAutomaticLoop":true,
                    "actualFallback":case!="pending","pendingOnly":case=="pending","quietObservationMs":500}),
                &mut stages, &mut captures).await?;
            click(desktop, "Retry description", true).await?;
            let retry = request(state, "directory", &recovery::ids()).await?;
            let before = finished(state);
            recovery::release(retry, Ok(directory(&recovery::ids())))?;
            settled(state, before).await?;
        }
    }
    retained(state)?;
    editor(desktop).await?;
    let model = recovery::snapshot(&state.peek());
    if recovery::ids()
        .iter()
        .any(|id| model["groups"][id]["source"] != recovery::raw(id))
    {
        return Err(format!(
            "Automatic read did not admit original labels: {model}"
        ));
    }
    if case == "ready" && !recovery::requests().is_empty() {
        return Err("Canonical ready group opening issued a read".into());
    }
    if case == "mixed-member" {
        let mixed = &model["mixedMember"];
        if mixed != &original["mixedMember"]
            || mixed["observation"]["current"] != true
            || ["absentDriver", "knownDriver"]
                .iter()
                .any(|key| mixed[*key] != json!({"started":0,"finished":0,"active":0}))
            || mixed["automaticDriver"]
                != json!({"pending":[],"subscribed":[],"stale":[],
                "worker":false,"started":0,"finished":0,"http_finished":0,"acknowledgments":0,"failures":0})
        {
            return Err(format!(
                "Original known member changed or triggered a read: {mixed}"
            ));
        }
    }
    capture(state, desktop, name, "success",
        json!({"actualReadAndAdoption":case!="ready","readyWithoutRead":case=="ready","originalDraft":true}),
        &mut stages, &mut captures).await?;

    // Exercise the actual original editable leaf/input path; retain opaque tokens.
    let edited = wait_dom(desktop, &format!(r#"(()=>{{
        const e=document.getElementById('profile-section-rich');
        const leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');
        if(!e?.isContentEditable||!leaf)return{{ok:false}};
        const before=window.superPlatinumRichEditor.resolved(e);
        leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));
        const after=window.superPlatinumRichEditor.resolved(e);
        return{{ok:true,before,after}};
    }})()"#, json!(recovery::EDITED))).await?;
    editor_text(desktop, recovery::EDITED).await?;
    retained(state)?;
    capture(
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
        return Err("Use description lost actual native edit".into());
    }
    let count = recovery::requests().len();
    save_text(state, desktop, recovery::EDITED).await?;
    capture(
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
    if recovery::requests().len() != count {
        return Err("Canonical cache reopening repeated group acquisition".into());
    }
    capture(
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
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}

async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    held: bool,
) -> Result<(), String> {
    let requests = recovery::requests();
    let groups = recovery::snapshot(&state.peek())["groups"].clone();
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if held {
            loading(desktop).await?;
        } else {
            unavailable(desktop).await?;
        }
        retained(state)?;
        if recovery::requests() != requests || recovery::snapshot(&state.peek())["groups"] != groups
        {
            return Err(format!(
                "Automatic opening looped or changed source: {}",
                recovery::snapshot(&state.peek())
            ));
        }
    }
    Ok(())
}
