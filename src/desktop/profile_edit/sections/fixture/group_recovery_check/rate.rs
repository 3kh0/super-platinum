//! Actual elapsed endpoint debt, original unopened draft and explicit recovery.
use super::scenario::{
    directory, finished, loading, request, retained, save_text, settled, unchanged,
};
use super::*;
use std::time::Instant;

const DIRECTORY_SECONDS: u64 = 8;
const DIRECT_SECONDS: u64 = 15;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
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
    let first = request(state, "directory", &recovery::ids()).await?;
    loading(desktop).await?;
    quiet(state, &original, 500).await?;
    capture(
        state,
        desktop,
        name,
        "held",
        json!({"automaticRead":true,"duplicateIdsCoalesced":true,"duplicateOpen":true,
            "noAutomaticLoop":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    let before = finished(state);
    recovery::release(
        first,
        Err(Error::RateLimited {
            retry_after_secs: Some(DIRECTORY_SECONDS),
        }),
    )?;
    let direct = request(state, "direct", &recovery::ids()).await?;
    loading(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "direct-held",
        json!({"independentFallback":true,"directoryRequest":first,
            "directoryRetryAfterSeconds":DIRECTORY_SECONDS}),
        &mut stages,
        &mut captures,
    )
    .await?;
    recovery::release(
        direct,
        Err(Error::RateLimited {
            retry_after_secs: Some(DIRECT_SECONDS),
        }),
    )?;
    settled(state, before).await?;
    unchanged(state, &original)?;
    limited(desktop, true).await?;
    let disabled = disabled_retry(desktop).await?;
    quiet(state, &original, 500).await?;
    capture(
        state,
        desktop,
        name,
        "limited",
        json!({"sourceRetained":true,"draftRetained":true,"disabledRetry":disabled,
            "noDispatch":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    click(desktop, "Cancel description edit", false).await?;
    form(desktop).await?;
    unchanged(state, &original)?;
    capture(
        state,
        desktop,
        name,
        "cancelled",
        json!({"draftRetained":true,"debtRetained":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let requests = recovery::requests();
    click(desktop, "Edit description", true).await?;
    limited(desktop, true).await?;
    let disabled = disabled_retry(desktop).await?;
    quiet(state, &original, 500).await?;
    if recovery::requests() != requests {
        return Err(format!(
            "Reopening during debt dispatched: {}",
            recovery::snapshot(&state.peek())
        ));
    }
    capture(
        state,
        desktop,
        name,
        "reopened-limited",
        json!({"draftRetained":true,"debtRetained":true,"duplicateOpen":true,
            "disabledRetry":disabled,"noDispatch":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    // The first endpoint becomes available independently of the second.
    // Wait on real retained state; never alter clocks/deadlines.
    wait_expiry(state, &requests).await?;
    limited(desktop, false).await?;
    quiet(state, &original, 500).await?;
    let elapsed = {
        let shell = state.peek();
        let team = shell
            .core
            .active_team
            .as_deref()
            .ok_or("Missing rate owner")?;
        json!({
            "directoryElapsedMs":crate::usergroups::recovery::rate_elapsed_ms(&shell,team,"directory"),
            "directElapsedMs":crate::usergroups::recovery::rate_elapsed_ms(&shell,team,"direct"),
        })
    };
    capture(
        state,
        desktop,
        name,
        "expired",
        json!({"realExpiry":true,"noAutomaticRequest":true,"quietObservationMs":500,
            "elapsed":elapsed}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Retry description", true).await?;
    let retry = request(state, "directory", &recovery::ids()).await?;
    let before = finished(state);
    recovery::release(retry, Ok(directory(&recovery::ids())))?;
    settled(state, before).await?;
    retained(state)?;
    editor(desktop).await?;
    let model = recovery::snapshot(&state.peek());
    if recovery::ids()
        .iter()
        .any(|id| model["groups"][id]["source"] != recovery::raw(id))
    {
        return Err(format!(
            "Rate recovery did not adopt original labels: {model}"
        ));
    }
    capture(
        state,
        desktop,
        name,
        "success",
        json!({"actualReadAndAdoption":true,"originalDraft":true,"explicitRetry":true}),
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
        return Err("Rate recovery Use description lost native edit".into());
    }
    let requests = recovery::requests();
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
    if recovery::requests() != requests {
        return Err("Rate recovery warm reopening issued a request".into());
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
        json!({"ok":true,"fixture":name,"case":"rate-limit","readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}

async fn disabled_retry(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop, r#"(()=>{
        const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==='Retry description');
        if(!b||!b.disabled)return{ok:false};b.click();b.click();
        return{ok:true,disabled:true,clicks:2};
    })()"#).await
}

async fn quiet(state: Signal<ShellState>, original: &Value, millis: u64) -> Result<(), String> {
    let requests = recovery::requests();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(millis) {
        unchanged(state, original)?;
        if recovery::requests() != requests {
            return Err(format!(
                "Rate debt automatically dispatched: {}",
                recovery::snapshot(&state.peek())
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

async fn limited(desktop: &DesktopContext, disabled: bool) -> Result<(), String> {
    wait_dom(desktop, &format!(r#"(()=>{{
        const status=document.getElementById('profile-section-rich-status');
        const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==='Retry description');
        const expected={};
        return{{ok:!document.getElementById('profile-section-rich')&&status?.textContent===expected&&!!b&&b.disabled==={disabled},
            text:status?.textContent}};
    }})()"#,json!(if disabled {"Slack asked us to wait before loading group names. Your description is unchanged."} else {"You can retry loading group names now. Your description is unchanged."}))).await.map(|_| ())
}

async fn wait_expiry(state: Signal<ShellState>, requests: &[Value]) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(25) {
        let model = recovery::snapshot(&state.peek());
        if recovery::requests() != requests {
            return Err(format!("Debt expiry dispatched without Retry: {model}"));
        }
        if model["rate"]["directory"]["blocked"] == false {
            if model["rate"]["direct"]["blocked"] != true || model["rate"]["blocked"] != false {
                return Err(format!(
                    "Independent directory expiry was not observed before direct expiry: {model}"
                ));
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Endpoint debt did not expire: {}",
        recovery::snapshot(&state.peek())
    ))
}
