//! Sequential cold file/group admission with an already canonical member.
use super::super::files;
use super::scenario::{directory, finished, request, retained, save_text, settled, unchanged};
use super::*;

const LOADING: &str = "Loading reference details. Your description is unchanged.";
const UNAVAILABLE: &str = "Reference details are unavailable or changed while opening. Your description is unchanged. Retry to load document and mention details.";

pub(super) async fn message(desktop: &DesktopContext, loading: bool) -> Result<(), String> {
    let text = if loading { LOADING } else { UNAVAILABLE };
    wait_dom(desktop, &format!(r#"(()=>{{
        const status=document.getElementById('profile-section-rich-status');
        const retry=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==='Retry description');
        return{{ok:!document.getElementById('profile-section-rich')&&status?.textContent==={}&&!!retry&&retry.disabled==={loading}}};
    }})()"#, json!(text))).await.map(|_|())
}

pub(super) async fn file_request(count: usize, completed: usize, held: bool) -> Result<(), String> {
    for _ in 0..100 {
        let value = files::status();
        if value["requests"].as_array().map(Vec::len) == Some(count)
            && value["completed"] == completed
            && value["held"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Mixed file read did not settle: {}",
        files::status()
    ))
}

fn member(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    let value = &model["mixedMember"];
    if value != &original["mixedMember"]
        || value["observation"]["current"] != true
        || ["absentDriver", "knownDriver"]
            .iter()
            .any(|key| value[*key] != json!({"started":0,"finished":0,"active":0}))
        || value["automaticDriver"]
            != json!({"pending":[],"subscribed":[],"stale":[],
            "worker":false,"started":0,"finished":0,"http_finished":0,"acknowledgments":0,"failures":0})
    {
        return Err(format!("Mixed known member changed or issued I/O: {value}"));
    }
    Ok(())
}

fn opening(state: Signal<ShellState>, expected: &Value) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    let actual = &model["mixedFile"]["opening"];
    if ["visit", "serial", "key", "reading"]
        .iter()
        .any(|key| actual[*key] != expected[*key])
        || actual["mounted"] != false
    {
        return Err(format!(
            "Original unopened rich source replaced: expected {expected}; actual {actual}"
        ));
    }
    retained(state)
}

fn retracked(previous: &Value, actual: &Value, reading: bool, mounted: bool) -> Result<(), String> {
    if ["visit", "serial"]
        .iter()
        .any(|key| actual[*key] != previous[*key])
        || !actual["key"]
            .as_u64()
            .zip(previous["key"].as_u64())
            .is_some_and(|(next, prior)| next > prior)
        || actual["reading"] != reading
        || actual["mounted"] != mounted
    {
        return Err(format!(
            "Mixed admission did not retain its source while retracking: previous {previous}; actual {actual}; expected reading {reading}, mounted {mounted}"
        ));
    }
    Ok(())
}

pub(super) fn ready_file(state: Signal<ShellState>) -> Result<(), String> {
    let model = recovery::snapshot(&state.peek());
    let cache = &model["mixedFile"]["cache"];
    if cache["current"] != true || cache["source"] != files::raw(false)["file"] {
        return Err(format!(
            "Original acquired file source unavailable: {cache}"
        ));
    }
    Ok(())
}

async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    loading: bool,
    original: &Value,
    first: &Value,
) -> Result<(), String> {
    let before = recovery::snapshot(&state.peek());
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        message(desktop, loading).await?;
        member(state, original)?;
        opening(state, first)?;
        let after = recovery::snapshot(&state.peek());
        if after != before {
            return Err(format!(
                "Mixed unopened quiet state changed: before {before}; after {after}"
            ));
        }
    }
    Ok(())
}

async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture(state, desktop, name, stage, proof, stages, captures).await?;
    let geometry = &stages.last().ok_or("Missing mixed stage")?["geometry"];
    if geometry["richEditable"] == true {
        let tokens = geometry["tokens"]
            .as_array()
            .ok_or("Missing mixed tokens")?;
        if tokens.len() != 6
            || tokens[4]["fontWeight"] != "700"
            || tokens[4]["fontStyle"] != "normal"
            || tokens[5]["fontWeight"] != "400"
            || tokens[5]["fontStyle"] != "italic"
        {
            return Err(format!("Mixed file computed styles differ: {tokens:?}"));
        }
    }
    Ok(())
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
) -> Result<Value, String> {
    recovery::seed(state).await?;
    let original = recovery::snapshot(&state.peek());
    if !files::enabled(&state.peek())
        || !files::list()
        || !original["mixedFile"]["cache"].is_null()
        || files::status()["requests"] != json!([])
    {
        return Err(format!("Mixed original file must be cold: {original}"));
    }
    member(state, &original)?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    retained(state)?;
    shot(
        state,
        desktop,
        name,
        "initial",
        json!({"originalDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;

    files::mode(files::Mode::Held);
    click(desktop, "Edit description", true).await?;
    file_request(1, 0, true).await?;
    message(desktop, true).await?;
    let first = recovery::snapshot(&state.peek())["mixedFile"]["opening"].clone();
    if first["key"].as_u64().is_none_or(|key| key == 0) || !recovery::requests().is_empty() {
        return Err("Mixed opening dispatched groups before file completion".into());
    }
    quiet(state, desktop, true, &original, &first).await?;
    shot(
        state,
        desktop,
        name,
        "file-held",
        json!({"automaticRead":true,"duplicateOpen":true,"openingRetained":true,
            "memberRetained":true,"noAutomaticLoop":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    let released = files::release();
    if released != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err(format!(
            "Original mixed file response not delivered: {released}"
        ));
    }
    file_request(1, 1, false).await?;
    let pending = request(state, "directory", &recovery::ids()).await?;
    ready_file(state)?;
    quiet(state, desktop, true, &original, &first).await?;
    shot(
        state,
        desktop,
        name,
        "group-held",
        json!({"automaticRead":true,"fileReady":true,"draftRetained":true,
            "openingRetained":true,"memberRetained":true,"fileRelease":released,
            "noAutomaticLoop":true,"quietObservationMs":500}),
        &mut stages,
        &mut captures,
    )
    .await?;

    let before = finished(state);
    recovery::release(
        pending,
        Err(Error::Offline("closed mixed group directory".into())),
    )?;
    let fallback = request(state, "direct", &recovery::ids()).await?;
    recovery::release(
        fallback,
        Err(Error::Offline("closed mixed group fallback".into())),
    )?;
    settled(state, before).await?;
    unchanged(state, &original)?;
    ready_file(state)?;
    message(desktop, false).await?;
    // Recovery settles read bookkeeping successfully, then admission retries
    // every original token. Missing groups leave a new, unopened rich key.
    let offline = recovery::snapshot(&state.peek())["mixedFile"]["opening"].clone();
    retracked(&first, &offline, false, false)?;
    quiet(state, desktop, false, &original, &offline).await?;
    shot(state, desktop, name, "offline",
        json!({"sourceRetained":true,"draftRetained":true,"actualFallback":true,
            "openingRetained":true,"memberRetained":true,"noAutomaticLoop":true,"quietObservationMs":500}),
        &mut stages, &mut captures).await?;

    let group_requests = recovery::requests();
    click(desktop, "Retry description", true).await?;
    file_request(2, 1, true).await?;
    message(desktop, true).await?;
    let retry = recovery::snapshot(&state.peek())["mixedFile"]["opening"].clone();
    retracked(&offline, &retry, true, false)?;
    if recovery::requests() != group_requests {
        return Err("Explicit mixed Retry did not retain a fresh file-first opening".into());
    }
    ready_file(state)?;
    quiet(state, desktop, true, &original, &retry).await?;
    shot(state, desktop, name, "retry-file-held",
        json!({"explicitRetryRead":true,"duplicateRetry":true,"draftRetained":true,
            "openingRetained":true,"memberRetained":true,"noAutomaticLoop":true,"quietObservationMs":500}),
        &mut stages, &mut captures).await?;
    let released = files::release();
    if released != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err(format!(
            "Forced mixed file response not delivered: {released}"
        ));
    }
    file_request(2, 2, false).await?;
    let pending = request(state, "directory", &recovery::ids()).await?;
    let before = finished(state);
    recovery::release(pending, Ok(directory(&recovery::ids())))?;
    settled(state, before).await?;
    editor(desktop).await?;
    retained(state)?;
    ready_file(state)?;
    member(state, &original)?;
    let success = recovery::snapshot(&state.peek())["mixedFile"]["opening"].clone();
    retracked(&retry, &success, false, true)?;
    for id in recovery::ids() {
        if recovery::snapshot(&state.peek())["groups"][&id]["source"] != recovery::raw(&id) {
            return Err(format!("Mixed successful group source changed: {id}"));
        }
    }
    shot(
        state,
        desktop,
        name,
        "success",
        json!({"actualReadAndAdoption":true,"originalDraft":true,"fileRelease":released}),
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
        return Err("Use description changed mixed opaque tokens".into());
    }
    let group_requests = recovery::requests();
    let file_requests = files::status();
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
    member(state, &original)?;
    if recovery::requests() != group_requests || files::status() != file_requests {
        return Err("Mixed canonical reopening repeated metadata acquisition".into());
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
        "requests":recovery::requests(),"captureChecksPassed":true,"visualReview":"required"}),
    )
}
