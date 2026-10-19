//! Native acceptance through actual Profile rich capture and section-save handlers.
use super::mentions::{self, FLAG};
use super::*;
use crate::slack_links_check::wait_dom as native_wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;
mod files;
mod groups;

pub(super) async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_PROFILE_MENTIONS_NONCE")
        .map_err(|_| "Missing profile mentions readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid profile mentions readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_PROFILE_MENTIONS_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute profile mentions readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("profile-sections-mentions-{nonce}").as_str())
    {
        return Err("Wrong profile mentions readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect profile mentions readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid profile mentions readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect profile mentions readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid profile mentions readiness marker".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "Read profile mentions readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse profile mentions readiness marker".into())
        })
        .await
        .map_err(|_| "Profile mentions readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Profile mentions readiness marker must be fresh".into());
    }
    eprintln!("profile-sections-mentions-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong profile mentions readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Profile mentions readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if super::files::named() {
        files::start(state, desktop);
        return;
    }
    if super::completion::named() && std::env::var(super::completion::FLAG).as_deref() == Ok("1") {
        if super::completion::files::named() {
            super::file_completion_check::start(state, desktop);
        } else if std::env::var(super::completion::groups::FLAG).as_deref() == Ok("1") {
            super::group_completion_check::start(state, desktop);
        } else if std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK").as_deref()
            == Ok("1")
        {
            super::member_completion_check::start(state, desktop);
        } else {
            super::completion_check::start(state, desktop);
        }
        return;
    }
    if !mentions::named() || std::env::var(FLAG).as_deref() != Ok("1") {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"error":e,"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => json!({"ok":false,"fixture":name,"error":"Profile mentions timed out"}),
        };
        eprintln!("profile-sections-mentions-check: {result}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !mentions::enabled(&state.peek()) {
        return Err("Closed mentions identity missing".into());
    }
    mentions::seed(state, mentions::source_label()).await?;
    select_values(state, desktop).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    click(desktop, "Edit description", false).await?;
    editor(desktop, "Working with ", true).await?;
    mentions::seed(state, mentions::renamed_label()).await?;
    let (source_name, native_name) = {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().unwrap();
        if mentions::group_mode() {
            let group = mentions::group_row(&shell).ok_or("Missing observed fixture group")?;
            (
                group
                    .label_source
                    .as_ref()
                    .and_then(|r| r["handle"].as_str())
                    .map(str::to_owned),
                Some(group.handle.clone()),
            )
        } else if mentions::user_mode() {
            let account = shell
                .core
                .active_account
                .as_deref()
                .ok_or("Missing member account")?;
            let session = shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&team))
                .ok_or("Missing member session")?;
            let workspace = &shell.core.workspaces[&team];
            let subset = workspace
                .directory_members
                .subset(
                    account,
                    session,
                    shell.core.account_epoch,
                    "profile-mentions-fixture",
                    &std::collections::BTreeSet::from([mentions::USER.into()]),
                    &workspace.users,
                )
                .map_err(|e| e.to_string())?;
            (subset.rows()[0].source["profile"]["display_name"].as_str().map(str::to_owned),
                serde_json::to_value(&workspace.users[mentions::USER]).unwrap()["profile"]["display_name"].as_str().map(str::to_owned))
        } else {
            let source = crate::app_home::channel_fetches::canonical::capture(&mut shell, &team)
                .map_err(|e| e.to_string())?;
            (
                source
                    .row(mentions::CHANNEL)
                    .and_then(|r| r["name"].as_str())
                    .map(str::to_owned),
                shell.core.workspaces[&team].channels[mentions::CHANNEL]
                    .name
                    .clone(),
            )
        }
    };
    if source_name.as_deref() != Some(mentions::renamed_label())
        || native_name.as_deref() != Some(mentions::renamed_label())
    {
        return Err("Mounted label test did not actually rename canonical/native mention".into());
    }
    editor(desktop, "Working with ", true).await?;
    capture(
        state,
        desktop,
        name,
        "opened",
        json!({"canonicalLabel":true,"mountedLabelStable":true,"sourceName":source_name,
            "nativeName":native_name,"mountedLabel":mentions::mounted_label(),"rawTokenCount":2}),
        &mut stages,
        &mut captures,
    )
    .await?;

    mentions::seed(state, mentions::source_label()).await?;
    edit(desktop, "Saved with ").await?;
    use_description(state, desktop, "Saved with ").await?;
    save(state, desktop, Outcome::Saved, 1, 1).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, "Saved with ", true).await?;
    capture(
        state,
        desktop,
        name,
        "confirmed",
        json!({"confirmedSave":true,"duplicateGuard":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Cancel description edit", false).await?;

    click(desktop, "Edit description", false).await?;
    edit(desktop, "Rejected with ").await?;
    use_description(state, desktop, "Rejected with ").await?;
    save(state, desktop, Outcome::Rejected, 2, 1).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, "Rejected with ", true).await?;
    capture(
        state,
        desktop,
        name,
        "rejected",
        json!({"rejectionRetainsDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    edit(desktop, "Uncertain with ").await?;
    use_description(state, desktop, "Uncertain with ").await?;
    save(state, desktop, Outcome::Uncertain, 3, 1).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.profile-edit-notice')?.textContent==='Slack has not confirmed this change. Check your profile before saving again.'})"#).await?;
    capture(
        state,
        desktop,
        name,
        "uncertain",
        json!({"uncertainRetainsDraft":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Check profile", false).await?;
    counts(state, 3, 2, false, true).await?;
    click(desktop, "Check profile", false).await?;
    counts(state, 3, 3, false, true).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.profile-edit-notice')?.textContent==='Slack currently shows different values. Check again, or keep the current section without sending another change.'})"#).await?;
    capture(
        state,
        desktop,
        name,
        "checked",
        json!({"repeatedCheckNoReplay":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Keep current section", false).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-sections-title')&&!!document.getElementById('profile-edit-form')})"#).await?;
    let visit = state.peek().profile_edit.draft.as_ref().unwrap().visit;
    super::super::open(state, visit);
    click(desktop, "About you", false).await?;
    counts(state, 3, 3, false, false).await?;
    let proof = mentions::refuse_stale_opening(&mut state.write())?;
    if proof
        .as_object()
        .is_none_or(|p| p.values().any(|v| v != true))
    {
        return Err("Stale opening accepted or draft changed".into());
    }
    click(desktop, "Edit description", false).await?;
    unavailable(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "stale-metadata",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Retry description", false).await?;
    unavailable(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "unavailable",
        json!({"retryRetainsDraft":true,"noWrite":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    mentions::seed(state, mentions::source_label()).await?;
    click(desktop, "Retry description", false).await?;
    editor(desktop, "Saved with ", true).await?;
    capture(
        state,
        desktop,
        name,
        "retried",
        json!({"retryOpened":true,"draftRetained":true}),
        &mut stages,
        &mut captures,
    )
    .await?;

    if mentions::group_mode() {
        groups::run(state, desktop, name, &mut stages, &mut captures).await?;
    }
    // Adjacent equal-ID/label/style mention inserts merge in the source Delta.
    // Both raw opaque occurrences must nevertheless survive native capture.
    let limit = "x".repeat(4990);
    edit(desktop, &limit).await?;
    use_description(state, desktop, &limit).await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, &limit, true).await?;
    capture(
        state,
        desktop,
        name,
        "limit5000",
        json!({"sourceLength":5000,"accepted":true,"rawTokenCount":2}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let excess = "x".repeat(4991);
    edit(desktop, &excess).await?;
    // Existing field semantics retain invalid edits for correction; validation
    // blocks Save instead of discarding the submitted description.
    use_description(state, desktop, &excess).await?;
    let invalid = mentions::snapshot(&state.peek());
    if invalid["canSave"] != false || invalid["attempts"].as_array().map(Vec::len) != Some(3) {
        return Err("Oversize description was writable or sent".into());
    }
    click(desktop, "Edit description", false).await?;
    editor(desktop, &excess, true).await?;
    click(desktop, "Cancel description edit", false).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.profile-edit-field-error')?.textContent==="Check this field's value or length."&&[...document.querySelectorAll('.profile-edit button')].some(b=>b.textContent==='Save section'&&b.disabled)})"#).await?;
    capture(
        state,
        desktop,
        name,
        "limit5001",
        json!({"sourceLength":5001,"retainedInvalid":true,"saveDisabled":true,"reopenedExact":true,"rawTokenCount":2}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let proof = mentions::replace_account(state)?;
    if proof["oldOpeningRefused"] != true || proof["oldEditorHidden"] != true {
        return Err("Old account opening survived replacement".into());
    }
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})"#,
    )
    .await?;
    capture(
        state,
        desktop,
        name,
        "account-replaced",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"readiness":ready,"stages":stages,"captures":captures,
        "viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,
        "mentionKind":if mentions::group_mode(){"usergroup"}else if mentions::user_mode(){"user"}else{"channel"},
        "visualReview":"PNG inspection required separately"}),
    )
}

pub(super) async fn click(
    desktop: &DesktopContext,
    label: &str,
    twice: bool,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=[...document.querySelectorAll('.profile-edit button')].find(b=>b.textContent==={});if(!b||b.disabled)return{{ok:false}};b.scrollIntoView({{block:'nearest'}});const r=b.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==b&&!b.contains(hit))return{{ok:false}};b.click();if({twice})b.click();return{{ok:true}};}})()"#,json!(label))).await.map(|_|()).map_err(|e|format!("Click {label}: {e}"))
}
async fn editor(desktop: &DesktopContext, text: &str, editable: bool) -> Result<(), String> {
    editor_label(desktop, text, editable, &mentions::mounted_label()).await
}
async fn editor_label(
    desktop: &DesktopContext,
    text: &str,
    editable: bool,
    label: &str,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);return{{ok:!!v&&e.isContentEditable==={editable}&&v.snapshot.blocks[0].elements[0].elements[0].text==={}&&v.tokens.length===2&&v.tokens.every(t=>t.label==={})&&v.tokens.map(t=>t.value.fixture_occurrence).join(',')==='1,2'}};}})()"#,json!(text),json!(label))).await.map(|_|()).map_err(|e|format!("Editor content (prefix {:?}, length {}, editable {editable}): {e}", text.chars().take(32).collect::<String>(), text.len()))
}
async fn edit(desktop: &DesktopContext, text: &str) -> Result<(), String> {
    edit_label(desktop, text, &mentions::mounted_label()).await
}
async fn edit_label(desktop: &DesktopContext, text: &str, label: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');if(!e?.isContentEditable||!leaf)return{{ok:false}};leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return{{ok:true}};}})()"#,json!(text))).await?;
    editor_label(desktop, text, true, label).await
}
async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'})"#).await.map(|_|())
}
async fn use_description(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
) -> Result<(), String> {
    click(desktop, "Use description", false).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})"#).await?;
    if mentions::snapshot(&state.peek())["blocks"] != json!(mentions::blocks(text)) {
        return Err("Accepted description changed opaque tokens".into());
    }
    Ok(())
}
async fn counts(
    state: Signal<ShellState>,
    attempts: usize,
    reads: usize,
    busy: bool,
    uncertain: bool,
) -> Result<(), String> {
    for _ in 0..100 {
        let v = mentions::snapshot(&state.peek());
        if v["attempts"]
            .as_array()
            .is_some_and(|a| a.len() == attempts)
            && v["reads"] == reads
            && v["busy"] == busy
            && v["uncertain"] == uncertain
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Err(format!(
        "Wrong attempt/read/lock state: {}",
        mentions::snapshot(&state.peek())
    ))
}
pub(super) async fn save(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    outcome: Outcome,
    attempts: usize,
    reads: usize,
) -> Result<(), String> {
    state
        .write()
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .outcome = outcome;
    click(desktop, "Save section", true).await?;
    counts(
        state,
        attempts,
        if matches!(outcome, Outcome::Saved) {
            reads - 1
        } else {
            reads
        },
        true,
        true,
    )
    .await?;
    complete(&mut state.write());
    counts(
        state,
        attempts,
        reads,
        false,
        matches!(outcome, Outcome::Uncertain),
    )
    .await?;
    // Readback replaces the keyed SectionEditor. A prior still-enabled button
    // can enqueue a stale-serial callback before that replacement is painted.
    let view = presentation(&state.peek()).ok_or("Missing post-save section")?;
    let expected = json!({"visit":view.visit.to_string(),"serial":view.serial.to_string(),
        "locked":(view.busy||view.uncertain).to_string(),"notice":view.message});
    wait_dom(desktop,&format!(r#"(()=>{{const expected={expected},d=document.getElementById('shell-overlay-dialog');return{{ok:!!d&&d.dataset.dialogGeneration===expected.visit&&d.dataset.sectionSerial===expected.serial&&d.dataset.sectionLocked===expected.locked&&document.querySelector('.profile-edit-notice')?.textContent===expected.notice,expected}};}})()"#)).await.map_err(|e|format!("Post-save render: {e}"))?;
    select_values(state, desktop).await
}

pub(super) async fn select_values(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<(), String> {
    let model = mentions::snapshot(&state.peek());
    if model["choiceValue"] != "Morning" || model["countryValue"] != "US" {
        return Err(format!("Untouched model selects changed: {model}"));
    }
    wait_dom(desktop, r#"(()=>{const choice=document.getElementById('profile-section-field-choice')?.value,country=document.querySelector('select[aria-label="Country (optional)"]')?.value;return{ok:choice==='Morning'&&country==='US',choice,country};})()"#)
        .await.map(|_|()).map_err(|e|format!("Untouched native selects: {e}"))
}

/// Each poll returns an owned diagnostic snapshot even while its condition is
/// false, rather than losing the exact DOM generation behind a generic timeout.
pub(super) async fn wait_dom(desktop: &DesktopContext, script: &str) -> Result<Value, String> {
    let wrapped = format!(
        r#"(()=>{{let measured;try{{measured=({script});}}catch(error){{measured={{ok:false,error:String(error)}};}}
      const d=document.getElementById('shell-overlay-dialog'),e=document.getElementById('profile-section-rich');
      return{{ok:true,measured,diagnostic:{{dialog:!!d,visit:d?.dataset.dialogGeneration,serial:d?.dataset.sectionSerial,locked:d?.dataset.sectionLocked,
        richPane:!!document.getElementById('profile-section-rich-pane'),richEditor:!!e,richEditable:e?.isContentEditable,
        text:e?.textContent?.slice(0,100),tokens:e?[...e.querySelectorAll('.rich-editor-token')].map(t=>t.textContent):[],
        buttons:d?[...d.querySelectorAll('button')].map(b=>({{text:b.textContent,disabled:b.disabled}})):[],
        notices:d?[...d.querySelectorAll('.profile-edit-notice,.profile-edit-field-error,#profile-section-rich-status')].map(n=>n.textContent):[],
        choice:document.getElementById('profile-section-field-choice')?.value,
        country:document.querySelector('select[aria-label="Country (optional)"]')?.value}}}};}})()"#
    );
    let mut last = Value::Null;
    for _ in 0..60 {
        last = native_wait_dom(desktop, &wrapped).await?;
        if last["measured"]["ok"] == true {
            return Ok(last["measured"].clone());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Profile DOM condition timed out; last measurement: {last}"
    ))
}

async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let script = format!(
        "((allowEmptyGroup)=>{LAYOUT})({})",
        mentions::group_mode() && stage == "group-empty"
    );
    let mut geometry = wait_layout(desktop, stage, &script).await?;
    if super::files::named() {
        files::styles(&geometry)?;
    }
    let model = mentions::snapshot(&state.peek());
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture path worker failed")?
        .map_err(|_| "Capture path inspection failed")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "profile-sections-mentions-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "profile-sections-mentions-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn wait_layout(desktop: &DesktopContext, stage: &str, script: &str) -> Result<Value, String> {
    let wrapped = format!("(()=>({{ok:true,measured:({script})}}))()");
    let mut last = Value::Null;
    for _ in 0..60 {
        last = wait_dom(desktop, &wrapped).await?["measured"].clone();
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Layout {stage} timed out; last measurement: {last}"
    ))
}
const LAYOUT: &str = r#"(()=>{
 const d=document.querySelector('.profile-edit'),body=d?.querySelector('.profile-edit-body'),e=document.getElementById('profile-section-rich');
 if(body)body.scrollTop=body.scrollHeight;if(e)e.scrollTop=e.scrollHeight;
 if(!e)document.getElementById('profile-section-field-long')?.closest('.profile-edit-field')?.scrollIntoView({block:'nearest'});
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 const visible=e=>{const s=getComputedStyle(e),fragments=[...e.getClientRects()].filter(r=>r.width>0&&r.height>0);return s.visibility==='visible'&&s.display!=='none'&&fragments.length>0&&fragments.every(r=>{const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);let unclipped=true;for(let p=e.parentElement;p;p=p.parentElement){const ps=getComputedStyle(p),pr=rect(p);if(/auto|scroll|hidden|clip/.test(ps.overflowY)&&(r.top<pr.top-1||r.bottom>pr.bottom+1))unclipped=false;if(/auto|scroll|hidden|clip/.test(ps.overflowX)&&(r.left<pr.left-1||r.right>pr.right+1))unclipped=false;}return unclipped&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&(hit===e||e.contains(hit));});};
 const controls=d?[...d.querySelectorAll('footer button,#profile-section-rich-pane > button')].map(e=>({text:e.textContent,disabled:e.disabled,visible:visible(e),rect:rect(e)})):[];
 const notices=d?[...d.querySelectorAll('.profile-edit-notice,.profile-edit-field-error,#profile-section-rich-status')].filter(e=>e.textContent.trim()).map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)})):[];
 const tokens=e?[...e.querySelectorAll('.rich-editor-token')].map(e=>({text:e.textContent,editable:e.isContentEditable,fontWeight:getComputedStyle(e).fontWeight,fontStyle:getComputedStyle(e).fontStyle,visible:visible(e),rect:rect(e),fragments:[...e.getClientRects()].map(r=>({left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}))})):[];
 const native=e&&window.superPlatinumRichEditor.resolved(e),locked=document.getElementById('profile-section-field-locked');
 const noOverflow=document.documentElement.scrollWidth<=innerWidth+1&&(!d||d.scrollWidth<=d.clientWidth+1);
 return{ok:noOverflow&&(!d||visible(d))&&controls.every(e=>e.visible)&&notices.every(e=>e.visible)&&tokens.every((t,i)=>t.visible||(allowEmptyGroup&&t.text===''&&t.rect.width===0&&native?.tokens[i]?.label===''&&native?.tokens[i]?.value?.type==='usergroup')),
 viewport:[innerWidth,innerHeight],noOverflow,dialog:!!d,dialogRect:d?rect(d):null,controls,notices,tokens,
 captured:native||null,richEditable:e?e.isContentEditable:null,managedFieldPreserved:locked?locked.disabled&&locked.value==='Preserved organization value':null,
 longDisabled:document.getElementById('profile-section-field-long')?.disabled??null,
 choiceValue:document.getElementById('profile-section-field-choice')?.value??null,
 countryValue:document.querySelector('select[aria-label="Country (optional)"]')?.value??null};
})()"#;
