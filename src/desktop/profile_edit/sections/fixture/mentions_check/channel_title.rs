//! Existing Profile controls with finite shared channel refresh responses.
use super::super::{channel_title as source, files as file, recovery};
use super::*;
use crate::app_home::channel_fetches::conditional::fixture as transport;
use crate::profile_edit::sections::rich;
mod membership;

pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":error,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Channel title native timeout"})
            }
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("profile-channel-title-check: {result}"))
                .await;
    });
}
async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let proof = json!({"source":source::snapshot(&state.peek()),"cache":rich::files::fixture_status(&state.peek(),file::ID),"exercise":exercise});
    capture(state, desktop, name, stage, proof, stages, captures).await
}
async fn request(
    state: Signal<ShellState>,
    requests: &mut Vec<Value>,
    method: &str,
) -> Result<u64, String> {
    for _ in 0..100 {
        let pending = transport::pending(&state.peek());
        if pending.len() > 1 {
            return Err("Duplicate channel title conditional request".into());
        }
        if let Some(p) = pending.first() {
            if p.method != method || p.ids != [mentions::CHANNEL] {
                return Err("Channel title request escaped exact nested dependency".into());
            }
            requests.push(json!({"serial":p.serial,"method":p.method,"ids":p.ids,"versions":p.versions,"predecessor":source::snapshot(&state.peek())}));
            return Ok(p.serial);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Expected conditional {method}: {}",
        source::snapshot(&state.peek())
    ))
}
async fn idle(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..100 {
        if transport::idle(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Channel title refresh did not settle: {}",
        source::snapshot(&state.peek())
    ))
}
async fn release(
    mut state: Signal<ShellState>,
    serial: u64,
    response: Result<Value, Error>,
    requests: &mut [Value],
) -> Result<(), String> {
    let record = requests
        .iter_mut()
        .find(|r| r["serial"] == serial)
        .ok_or("Unknown request serial")?;
    record["response"] = match &response {
        Ok(raw) => json!({"raw":raw}),
        Err(_) => json!({"error":"closed_failure"}),
    };
    if !transport::release(&mut state.write(), serial, response) {
        return Err("Held channel reply refused".into());
    }
    Ok(())
}
fn unchanged() -> Value {
    json!({"results":[],"member_channels":[]})
}
fn changed(name: &str, updated: u64) -> Value {
    json!({"results":[source::row(name,updated)],"member_channels":if source::initial_nonmember() { Vec::<&str>::new() } else { vec![mentions::CHANNEL] }})
}
fn require_name(state: Signal<ShellState>, name: &str) -> Result<(), String> {
    let s = source::snapshot(&state.peek());
    if s["channel"]["name"] != name || s["nativeName"] != name {
        return Err(format!("Channel source/native mismatch: {s}"));
    }
    Ok(())
}
async fn reopen_stale(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    label: &str,
) -> Result<(), String> {
    click(desktop, "Cancel description edit", false).await?;
    let before = source::snapshot(&state.peek());
    transport::resynced(state, vec![mentions::CHANNEL.into()]).map_err(|e| e.to_string())?;
    let after = source::snapshot(&state.peek());
    if before["channel"] != after["channel"] || !transport::pending(&state.peek()).is_empty() {
        return Err("Stale topic mutated predecessor or fetched without selector".into());
    }
    click(desktop, "Edit description", false).await?;
    editor_label(desktop, "Working with ", true, label).await
}
async fn file_held() -> Result<(), String> {
    for _ in 0..100 {
        if file::status()["held"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Expected held file title read".into())
}
async fn no_editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed channel title owner required".into());
    }
    select_values(state, desktop).await?;
    source::seed(state).await?;
    let case = source::case();
    let mut requests = Vec::new();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    file::mode(file::Mode::Held);
    click(desktop, "Edit description", false).await?;
    file_held().await?;
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
    if case == "cancel" {
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        // Wait for the original opening's cancellation to drop its receiver.
        for _ in 0..100 {
            if file::status()["cancelled"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let result = file::release();
        if result["cancelledBeforeReply"] != true || result["delivered"] != false {
            return Err("Cancelled file opening accepted its late reply".into());
        }
        shot(
            state,
            desktop,
            name,
            "file-cancelled",
            json!({"release":result}),
            &mut stages,
            &mut captures,
        )
        .await?;
        file::mode(file::Mode::Normal);
        click(desktop, "Edit description", false).await?;
    } else {
        let result = file::release();
        if result["delivered"] != true {
            return Err("Initial file reply not delivered".into());
        }
        file::mode(file::Mode::Normal);
    }
    editor_label(desktop, "Working with ", true, source::original_label()).await?;
    let initial = request(state, &mut requests, "channels.info").await?;
    shot(
        state,
        desktop,
        name,
        "opened",
        json!({"conditional":initial}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "membership" {
        membership::run(
            state,
            desktop,
            name,
            initial,
            &mut requests,
            &mut stages,
            &mut captures,
        )
        .await?;
    } else if case == "cancel" {
        edit_label(desktop, "Uncommitted draft with ", source::original_label()).await?;
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        release(
            state,
            initial,
            Ok(changed(source::renamed_name(), 102)),
            &mut requests,
        )
        .await?;
        idle(state).await?;
        require_name(state, source::renamed_name())?;
        if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
            return Err("Cancelled editor changed original form".into());
        }
        shot(
            state,
            desktop,
            name,
            "channel-cancelled",
            json!({"lateSharedPublication":true,"retiredEditorNotResumed":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Working with ", true, source::renamed_label()).await?;
        shot(
            state,
            desktop,
            name,
            "reopened",
            json!({"newOpening":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        release(state, initial, Ok(unchanged()), &mut requests).await?;
        idle(state).await?;
        shot(
            state,
            desktop,
            name,
            "initial-settled",
            json!({"initialConditionalRead":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        reopen_stale(state, desktop, source::original_label()).await?;
        let serial = request(state, &mut requests, "channels.info").await?;
        shot(
            state,
            desktop,
            name,
            "refresh-held",
            json!({"conditional":serial}),
            &mut stages,
            &mut captures,
        )
        .await?;
        if case == "retirement" {
            click(desktop, "Cancel description edit", false).await?;
            no_editor(desktop).await?;
            let opening = {
                let mut s = state.write();
                let v = presentation(&s).ok_or("Missing section")?;
                rich::begin(&mut s, v.visit, v.serial, "long", 9001).map_err(|e| e.to_string())?
            };
            release(
                state,
                serial,
                Ok(changed(source::renamed_name(), 102)),
                &mut requests,
            )
            .await?;
            idle(state).await?;
            if rich::finish(&mut state.write(), opening) {
                return Err("Pre-publication opening survived source change".into());
            }
            shot(
                state,
                desktop,
                name,
                "pre-mount-retired",
                json!({"oldOpeningRefused":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::renamed_label()).await?;
            reopen_stale(state, desktop, source::renamed_label()).await?;
            let old = request(state, &mut requests, "channels.info").await?;
            let socket = recovery::retire_socket(state)?;
            release(
                state,
                old,
                Ok(changed("retired-socket-name", 999)),
                &mut requests,
            )
            .await?;
            let successor = request(state, &mut requests, "channels.info").await?;
            release(state, successor, Ok(unchanged()), &mut requests).await?;
            idle(state).await?;
            require_name(state, source::renamed_name())?;
            editor_label(desktop, "Working with ", true, source::renamed_label()).await?;
            shot(
                state,
                desktop,
                name,
                "socket-retired",
                json!({"socket":socket,"latePublicationRefused":true,"successor":successor}),
                &mut stages,
                &mut captures,
            )
            .await?;
            // Reopen under the new socket: files.info also owns that socket.
            click(desktop, "Cancel description edit", false).await?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Working with ", true, source::renamed_label()).await?;
            reopen_stale(state, desktop, source::renamed_label()).await?;
            let old = request(state, &mut requests, "channels.info").await?;
            let account = mentions::replace_account(state)?;
            release(
                state,
                old,
                Ok(changed("retired-account-name", 1000)),
                &mut requests,
            )
            .await?;
            idle(state).await?;
            if account["oldOpeningRefused"] != true || account["oldEditorHidden"] != true {
                return Err("Account replacement retained old opening".into());
            }
            wait_dom(desktop,"({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})").await?;
            shot(
                state,
                desktop,
                name,
                "account-replaced",
                account,
                &mut stages,
                &mut captures,
            )
            .await?;
        } else {
            if case == "failure" {
                edit_label(desktop, "Retained draft with ", source::original_label()).await?;
                release(
                    state,
                    serial,
                    Err(Error::Api("closed_nested_channel_failure".into())),
                    &mut requests,
                )
                .await?;
                let fallback = request(state, &mut requests, "conversations.genericInfo").await?;
                release(
                    state,
                    fallback,
                    Err(Error::Api("closed_nested_channel_fallback".into())),
                    &mut requests,
                )
                .await?;
                idle(state).await?;
                editor_label(
                    desktop,
                    "Retained draft with ",
                    true,
                    source::original_label(),
                )
                .await?;
                wait_dom(desktop,"({ok:[...document.querySelectorAll('.profile-edit-notice,#profile-section-rich-status')].some(e=>e.textContent==='Channel metadata could not refresh.')})").await?;
                shot(
                    state,
                    desktop,
                    name,
                    "failure-retained",
                    json!({"nestedFeedback":true,"draftRetained":true}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
                // Preserve the edited text in the section draft. Reopening is
                // another ordinary selector encounter, not a new refresh API.
                use_description(state, desktop, "Retained draft with ").await?;
                transport::resynced(state, vec![mentions::CHANNEL.into()])
                    .map_err(|e| e.to_string())?;
                click(desktop, "Edit description", false).await?;
                editor_label(
                    desktop,
                    "Retained draft with ",
                    true,
                    source::original_label(),
                )
                .await?;
                let retry = request(state, &mut requests, "channels.info").await?;
                shot(
                    state,
                    desktop,
                    name,
                    "retry-held",
                    json!({"conditional":retry}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
                release(
                    state,
                    retry,
                    Ok(changed(source::renamed_name(), 102)),
                    &mut requests,
                )
                .await?;
            } else {
                release(
                    state,
                    serial,
                    Ok(changed(source::renamed_name(), 102)),
                    &mut requests,
                )
                .await?;
            }
            idle(state).await?;
            require_name(state, source::renamed_name())?;
            editor_label(
                desktop,
                if case == "failure" {
                    "Retained draft with "
                } else {
                    "Working with "
                },
                true,
                source::original_label(),
            )
            .await?;
            shot(
                state,
                desktop,
                name,
                "renamed-mounted",
                json!({"mountedLabelRetained":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            edit_label(desktop, "Saved with ", source::original_label()).await?;
            use_description(state, desktop, "Saved with ").await?;
            shot(
                state,
                desktop,
                name,
                "used",
                json!({"rawTokensRetained":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            save(state, desktop, Outcome::Saved, 1, 1).await?;
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, "Saved with ", true, source::renamed_label()).await?;
            shot(
                state,
                desktop,
                name,
                "confirmed",
                json!({"confirmedSave":true,"newOpeningLabel":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
    }
    let mut result = json!({"ok":true,"channelTitle":true,"case":case,"fixture":name,"readiness":ready,"mentionKind":"canvas","titleBlocks":true,
        "stages":stages,"captures":captures,"requests":requests,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"visualReview":"Original PNG inspection required separately"});
    if source::private() {
        result["privateChannelTitle"] = json!(true);
    }
    if source::nonmember() {
        result["nonmemberChannelTitle"] = json!(true);
    }
    Ok(result)
}
