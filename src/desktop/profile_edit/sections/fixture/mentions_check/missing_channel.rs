//! Actual Profile controls around the shared missing-selector worker.
use super::super::{files as file, missing_channel as source, recovery};
use super::*;
use crate::app_home::channel_fetches::{
    conditional::fixture as conditional, unknown::fixture as transport,
};
use crate::profile_edit::sections::rich;

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
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":e,"source":source::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Missing channel title native timeout"})
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
    known: bool,
) -> Result<u64, String> {
    for _ in 0..100 {
        let values = if known {
            conditional::pending(&state.peek())
                .into_iter()
                .map(|p| (p.serial, p.method, p.ids, p.versions))
                .collect::<Vec<_>>()
        } else {
            transport::pending(&state.peek())
                .into_iter()
                .map(|p| (p.serial, p.method, p.ids, p.versions))
                .collect::<Vec<_>>()
        };
        if values.len() > 1 {
            return Err("Duplicate nested channel acquisition".into());
        }
        if let Some((serial, m, ids, versions)) = values.first() {
            if m != method || ids != &vec![mentions::CHANNEL.to_owned()] {
                return Err("Wrong missing-channel request scope".into());
            }
            requests.push(json!({"manager":if known{"conditional"}else{"unknown"},"serial":serial,"method":m,"ids":ids,"versions":versions,"predecessor":source::snapshot(&state.peek())}));
            return Ok(*serial);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Expected {method}: {}",
        source::snapshot(&state.peek())
    ))
}
fn release(
    mut state: Signal<ShellState>,
    serial: u64,
    response: Result<Value, Error>,
    known: bool,
    requests: &mut [Value],
) -> Result<(), String> {
    let manager = if known { "conditional" } else { "unknown" };
    let row = requests
        .iter_mut()
        .rev()
        .find(|r| r["serial"] == serial && r["manager"] == manager)
        .ok_or("Unknown held request")?;
    row["response"] = match &response {
        Ok(v) => json!({"raw":v}),
        Err(_) => json!({"error":"closed_failure"}),
    };
    let accepted = if known {
        conditional::release(&mut state.write(), serial, response)
    } else {
        transport::release(&mut state.write(), serial, response)
    };
    if !accepted {
        return Err("Held missing-channel reply refused".into());
    }
    Ok(())
}
async fn idle(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..100 {
        if transport::idle(&state.peek()) && conditional::idle(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Missing-channel publication did not settle: {}",
        source::snapshot(&state.peek())
    ))
}
fn found() -> Value {
    json!({"results":[recovery::row("fetched-harbor",102)],"member_channels":[mentions::CHANNEL]})
}
async fn no_editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn file_held() -> Result<(), String> {
    for _ in 0..100 {
        if file::status()["held"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Expected held file title".into())
}
async fn settle_known(state: Signal<ShellState>, requests: &mut Vec<Value>) -> Result<(), String> {
    let p = request(state, requests, "channels.info", true).await?;
    release(
        state,
        p,
        Ok(json!({"results":[],"member_channels":[]})),
        true,
        requests,
    )?;
    idle(state).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed missing-channel owner required".into());
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
        for _ in 0..100 {
            if file::status()["cancelled"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let result = file::release();
        if result["cancelledBeforeReply"] != true || result["delivered"] != false {
            return Err("Cancelled file read resumed".into());
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
        if file::release()["delivered"] != true {
            return Err("File reply not delivered".into());
        }
        file::mode(file::Mode::Normal);
    }
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    let first = request(state, &mut requests, "channels.info", false).await?;
    if !source::snapshot(&state.peek())["channel"].is_null() {
        return Err("Transient selector manufactured a canonical row".into());
    }
    shot(
        state,
        desktop,
        name,
        "opened",
        json!({"immediateUnknown":true,"request":first}),
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
        // Retire the real source owner before its held positive reply. Boot
        // merges preserve acquired rows, so reseeding cannot manufacture a miss.
        let socket = recovery::retire_socket(state)?;
        release(state, first, Ok(found()), false, &mut requests)?;
        idle(state).await?;
        if !source::snapshot(&state.peek())["channel"].is_null() {
            return Err("Pre-mount retired owner published missing result".into());
        }
        if rich::finish(&mut state.write(), opening) {
            return Err("Stale provisional opening mounted".into());
        }
        shot(
            state,
            desktop,
            name,
            "pre-mount-retired",
            json!({"oldOpeningRefused":true,"socket":socket,"latePublicationRefused":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
        let old = request(state, &mut requests, "channels.info", false).await?;
        let socket = recovery::retire_socket(state)?;
        release(state, old, Ok(found()), false, &mut requests)?;
        idle(state).await?;
        if !source::snapshot(&state.peek())["channel"].is_null() {
            return Err("Old socket published missing result".into());
        }
        editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
        shot(
            state,
            desktop,
            name,
            "socket-retired",
            json!({"socket":socket,"latePublicationRefused":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Cancel description edit", false).await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
        let old = request(state, &mut requests, "channels.info", false).await?;
        let mut account = mentions::replace_account(state)?;
        let replacement = source::replacement_snapshot(&state.peek())?;
        if !replacement["workspaces"]
            .as_object()
            .is_some_and(|ws| ws.values().any(|w| w["pending"] == true))
        {
            return Err("Replacement fixture must retain its actual Boot queue".into());
        }
        release(state, old, Ok(found()), false, &mut requests)?;
        for _ in 0..100 {
            if transport::quiescent(&state.peek()) && conditional::idle(&state.peek()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !transport::quiescent(&state.peek()) || !conditional::idle(&state.peek()) {
            return Err("Retired channel work did not become quiescent".into());
        }
        let after = source::replacement_snapshot(&state.peek())?;
        if after != replacement || transport::idle(&state.peek()) {
            return Err("Late retired reply changed foreign pending source".into());
        }
        account["replacementBefore"] = replacement;
        account["replacementAfter"] = after;
        account["workersQuiescent"] = json!(true);
        account["globalIdle"] = json!(false);
        account["foreignPendingPreserved"] = json!(true);
        wait_dom(
            desktop,
            "({ok:!document.querySelector('.profile-edit')&&!!document.querySelector('.sidebar')})",
        )
        .await?;
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
    } else if case == "cancel" {
        edit_label(desktop, "Uncommitted draft with ", source::ORIGINAL).await?;
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        release(state, first, Ok(found()), false, &mut requests)?;
        idle(state).await?;
        if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
            return Err("Cancelled raw draft changed".into());
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
        editor_label(desktop, "Working with ", true, source::FOUND).await?;
        settle_known(state, &mut requests).await?;
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
        if case == "failure" {
            edit_label(desktop, "Retained draft with ", source::ORIGINAL).await?;
            release(
                state,
                first,
                Err(Error::Api("closed_unknown_failure".into())),
                false,
                &mut requests,
            )?;
            let fallback =
                request(state, &mut requests, "conversations.genericInfo", false).await?;
            release(
                state,
                fallback,
                Err(Error::Api("closed_unknown_fallback".into())),
                false,
                &mut requests,
            )?;
        } else {
            release(
                state,
                first,
                Ok(if case == "positive" {
                    found()
                } else {
                    json!({"results":[],"member_channels":[mentions::CHANNEL]})
                }),
                false,
                &mut requests,
            )?;
        }
        idle(state).await?;
        let lead = if case == "failure" {
            "Retained draft with "
        } else {
            "Working with "
        };
        editor_label(desktop, lead, true, source::ORIGINAL).await?;
        shot(
            state,
            desktop,
            name,
            if case == "failure" {
                "failure-retained"
            } else {
                "settled-mounted"
            },
            json!({"mountedLabelRetained":true,"localTerminalNotServerAbsence":case!="positive"}),
            &mut stages,
            &mut captures,
        )
        .await?;
        edit_label(desktop, "Saved with ", source::ORIGINAL).await?;
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
        editor_label(desktop, "Saved with ", true, source::final_label()).await?;
        settle_known(state, &mut requests).await?;
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
    Ok(
        json!({"ok":true,"channelTitle":true,"missingChannelTitle":true,"case":case,"fixture":name,"readiness":ready,
        "mentionKind":"canvas","titleBlocks":true,"stages":stages,"captures":captures,"requests":requests,
        "viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"visualReview":"Original PNG inspection required separately"}),
    )
}
