//! Unavailable opening follows shared same-account work; cancellation owns no IO.
use super::*;
const LATER: &str = "Later member source";

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
    case: &str,
) -> Result<Value, String> {
    recovery::seed(state).await?;
    let team = state.peek().core.active_team.clone().unwrap();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    macro_rules! stage {
        ($name:expr,$proof:expr) => {
            capture(
                state,
                desktop,
                name,
                $name,
                $proof,
                &mut stages,
                &mut captures,
            )
            .await?
        };
    }
    form(desktop).await?;
    stage!("initial", json!({"healthySource":true}));
    // Establish known subscription and capture a real native edited description.
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, recovery::LABEL).await?;
    let (request, frame) = pending(state, 101).await?;
    recovery::release(
        request,
        Ok((recovery::response(&team, recovery::LABEL, 101, false), None)),
    )?;
    recovery::acknowledge(frame, true)?;
    settle(state).await.map_err(|e| format!("Initial: {e}"))?;
    edit(desktop, recovery::EDITED).await?;
    click(desktop, "Use description", false).await?;
    form(desktop).await?;
    let event = recovery::changed(&mut state.write(), recovery::RENAMED, 102)?;
    if event["before"]["source"] != event["after"]["source"] {
        return Err("Socket changed canonical source".into());
    }
    if case == "cross-socket-opening" {
        stage!("changed-native", event);
    }
    click(desktop, "Edit description", false).await?;
    lifecycle::waiting(desktop).await?;
    let (request, frame) = pending(state, 101).await?;
    let original = recovery::snapshot(&state.peek())["opening"].clone();
    if original["reading"] != true || original["lease"] != true || original["mounted"] != false {
        return Err("Original missing-opening lease absent".into());
    }
    stage!(
        "opening-held",
        json!({"opening":original,"request":request,"frame":frame})
    );
    if case == "cross-socket-opening" {
        let before = recovery::snapshot(&state.peek());
        let generation = before["socketGeneration"].as_u64().unwrap();
        if !crate::realtime::mark_disconnected(&mut state.write(), &team, generation) {
            return Err("Accepted disconnect refused".into());
        }
        disconnected(state, &original).await?;
        lifecycle::waiting(desktop).await?;
        stage!(
            "disconnected",
            json!({"before":before,"opening":original,"acceptedGeneration":generation})
        );
        let connected = recovery::connect(&mut state.write())?;
        automatic::resume(state, &team);
        let (next_request, next_frame) =
            lifecycle::successor_pending(state, request, connected["socket"].as_u64().unwrap())
                .await?;
        unchanged_opening(state, &original)?;
        lifecycle::waiting(desktop).await?;
        stage!(
            "successor-held",
            json!({"connection":connected,"opening":original,"request":next_request,"frame":next_frame})
        );
        let delivered = recovery::release(
            request,
            Ok((
                recovery::response(&team, "Retired original poison", 999, false),
                None,
            )),
        )
        .is_ok();
        let old_ack = recovery::acknowledge(frame, true)?;
        if delivered || old_ack["routed"] != false {
            return Err("Retired original waiters remained live".into());
        }
        unchanged_opening(state, &original)?;
        stage!(
            "old-retired",
            json!({"opening":original,"oldHttpDelivered":delivered,"oldAck":old_ack})
        );
        let before_http = recovery::snapshot(&state.peek())["automatic"]["http_finished"]
            .as_u64()
            .unwrap();
        recovery::release(
            next_request,
            Ok((
                recovery::response(&team, recovery::RENAMED, 102, false),
                None,
            )),
        )?;
        http_finished(state, before_http).await?;
        unchanged_opening(state, &original)?;
        lifecycle::waiting(desktop).await?;
        stage!(
            "successor-read",
            json!({"opening":original,"beforeHttp":before_http})
        );
        let ack = recovery::acknowledge(next_frame, true)?;
        settle(state).await.map_err(|e| format!("Successor: {e}"))?;
        editor(desktop, recovery::EDITED, recovery::RENAMED).await?;
        source(state, recovery::RENAMED, 102)?;
        let after = continued(state, &original)?;
        stage!(
            "continued",
            json!({"opening":original,"afterOpening":after,"ack":ack,"noExtraOpen":true})
        );
    } else {
        recovery::release(
            request,
            Err(Error::Offline("closed unavailable opening fixture".into())),
        )?;
        let ack = recovery::acknowledge(frame, true)?;
        settle(state)
            .await
            .map_err(|e| format!("Failure settlement: {e}"))?;
        unavailable(desktop).await?;
        let failed = recovery::snapshot(&state.peek());
        if failed["opening"]["reading"] != false
            || failed["opening"]["lease"] != false
            || failed["opening"]["mounted"] != false
            || failed["source"]["updated"] != 101
        {
            return Err("Failed original opening retained loading or adopted source".into());
        }
        stage!(
            "failed",
            json!({"opening":original,"ack":ack,"quietFailure":true})
        );
        click(desktop, "Cancel description edit", false).await?;
        form(desktop).await?;
        recovery::changed(&mut state.write(), recovery::RENAMED, 102)?;
        click(desktop, "Edit description", false).await?;
        lifecycle::waiting(desktop).await?;
        let (request, frame) = pending(state, 101).await?;
        let canceled_original = recovery::snapshot(&state.peek())["opening"].clone();
        stage!(
            "retry-held",
            json!({"opening":canceled_original,"request":request,"frame":frame,"ordinaryOpen":true})
        );
        click(desktop, "Cancel description edit", false).await?;
        form(desktop).await?;
        let canceled = recovery::snapshot(&state.peek())["opening"].clone();
        if canceled["reading"] != false
            || canceled["lease"] != false
            || canceled["mounted"] != false
            || canceled["identity"] == canceled_original["identity"]
        {
            return Err("Cancel did not retire original consumer".into());
        }
        stage!(
            "canceled",
            json!({"beforeOpening":canceled_original,"afterOpening":canceled})
        );
        recovery::release(
            request,
            Ok((
                recovery::response(&team, recovery::RENAMED, 102, false),
                None,
            )),
        )?;
        let ack = recovery::acknowledge(frame, true)?;
        settle(state)
            .await
            .map_err(|e| format!("Independent shared completion: {e}"))?;
        source(state, recovery::RENAMED, 102)?;
        form(desktop).await?;
        if recovery::snapshot(&state.peek())["opening"] != canceled {
            return Err("Late result reopened canceled consumer".into());
        }
        stage!(
            "shared-finished",
            json!({"canceledOpening":canceled,"ack":ack,"noReopen":true})
        );
        let requests_before = recovery::requests().len();
        click(desktop, "Edit description", false).await?;
        editor(desktop, recovery::EDITED, recovery::RENAMED).await?;
        if recovery::requests().len() != requests_before {
            return Err("Fresh opening duplicated completed acquisition".into());
        }
        stage!("reopened", json!({"freshLabel":true}));
        click(desktop, "Use description", false).await?;
        form(desktop).await?;
        recovery::changed(&mut state.write(), LATER, 103)?;
        click(desktop, "Edit description", false).await?;
        lifecycle::waiting(desktop).await?;
        let (request, frame) = pending(state, 102).await?;
        let replacement = recovery::replace_account(state);
        let delivered = recovery::release(
            request,
            Ok((recovery::response(&team, LATER, 103, false), None)),
        )
        .is_ok();
        let ack = recovery::acknowledge(frame, true)?;
        wait_dom(desktop, "({ok:!document.querySelector('.profile-edit')})").await?;
        let after = recovery::snapshot(&state.peek());
        if after["native"] != Value::Null || after["sourcePresent"] == true {
            return Err("Foreign account acquired retired member".into());
        }
        stage!(
            "account-replaced",
            json!({"replacement":replacement,"lateHttpDelivered":delivered,"ack":ack})
        );
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "noLiveNetwork":true,"final":recovery::snapshot(&state.peek()),"visualReview":"required"}),
    )
}
fn unchanged_opening(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    let s = recovery::snapshot(&state.peek());
    if s["opening"] != *original
        || s["profile"]["blocks"] != json!(recovery::blocks(recovery::EDITED))
        || s["source"]["updated"] != 101
        || s["native"]["profile"]["display_name"] != recovery::RENAMED
    {
        return Err("Original unavailable opening/source drifted".into());
    }
    Ok(())
}
fn continued(state: Signal<ShellState>, original: &Value) -> Result<Value, String> {
    let s = recovery::snapshot(&state.peek());
    let after = &s["opening"];
    if after["reading"] != false
        || after["lease"] != false
        || after["mounted"] != true
        || after["visit"] != original["visit"]
        || after["serial"] != original["serial"]
        || !after["identity"]
            .as_u64()
            .zip(original["identity"].as_u64())
            .is_some_and(|(a, b)| a > b)
        || s["profile"]["blocks"] != json!(recovery::blocks(recovery::EDITED))
    {
        return Err("Continuation lost original draft custody".into());
    }
    Ok(after.clone())
}
async fn disconnected(state: Signal<ShellState>, original: &Value) -> Result<(), String> {
    for _ in 0..100 {
        unchanged_opening(state, original)?;
        let s = recovery::snapshot(&state.peek());
        if s["automatic"]["worker"] == false
            && s["automatic"]["pending"] == json!([recovery::MEMBER])
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Disconnected pending work did not settle into owned waiting gap".into())
}
async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Mention names are unavailable or changed while opening. Your description is unchanged. Retry to load member, channel, and group details.'&&[...document.querySelectorAll('.profile-edit button')].some(b=>b.textContent==='Retry description'&&!b.disabled)})"#).await.map(|_|())
}
