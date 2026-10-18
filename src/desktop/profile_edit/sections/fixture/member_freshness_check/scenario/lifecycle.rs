//! Additional lifetime cases; use the same accepted seed and native capture path.
use super::*;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    ready: Value,
    case: &str,
) -> Result<Value, String> {
    recovery::seed(state).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    form(desktop).await?;
    capture(
        state,
        desktop,
        name,
        "initial",
        json!({"healthySource":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::DRAFT, recovery::LABEL).await?;
    let (first_request, first_frame) = pending(state, 101).await?;
    let team = state.peek().core.active_team.clone().unwrap();
    if case == "socket-successor" {
        capture(
            state,
            desktop,
            name,
            "held",
            json!({"request":first_request,"frame":first_frame}),
            &mut stages,
            &mut captures,
        )
        .await?;
        edit(desktop, recovery::EDITED).await?;
        let native = remember(desktop).await?;
        capture(
            state,
            desktop,
            name,
            "native-edited",
            native,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = recovery::snapshot(&state.peek());
        let connection = recovery::connect(&mut state.write())?;
        automatic::resume(state, &team);
        // Observe real successor work before releasing either retired reply.
        let socket = connection["socket"]
            .as_u64()
            .ok_or("Missing successor socket")?;
        let (request, frame) = successor_pending(state, first_request, socket).await?;
        let native = custody(desktop).await?;
        source(state, recovery::LABEL, 101)?;
        capture(
            state,
            desktop,
            name,
            "socket-replaced",
            json!({"before":before,"connection":connection,
            "request":request,"frame":frame,"native":native}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let old_http_delivered = recovery::release(
            first_request,
            Ok((
                recovery::response(&team, "Retired reply must never publish", 999, false),
                None,
            )),
        )
        .is_ok();
        let old_ack = recovery::acknowledge(first_frame, true)?;
        if old_http_delivered || old_ack["routed"] != false {
            return Err(
                "Retired original read or ACK waiter still live after successor admission".into(),
            );
        }
        source(state, recovery::LABEL, 101)?;
        let native = custody(desktop).await?;
        capture(
            state,
            desktop,
            name,
            "retired-replies",
            json!({"oldHttpDelivered":old_http_delivered,"oldAck":old_ack,
            "successorRequest":request,"successorFrame":frame,"native":native}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let before_http = recovery::snapshot(&state.peek())["automatic"]["http_finished"]
            .as_u64()
            .unwrap();
        recovery::release(
            request,
            Ok((
                recovery::response(&team, recovery::RENAMED, 102, false),
                None,
            )),
        )?;
        http_finished(state, before_http).await?;
        let native = retained(state, desktop, recovery::LABEL, 101).await?;
        capture(
            state,
            desktop,
            name,
            "successor-read",
            json!({"native":native,"request":request,"frame":frame}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let ack = recovery::acknowledge(frame, true)?;
        settle(state)
            .await
            .map_err(|e| format!("Successor settlement: {e}"))?;
        source(state, recovery::RENAMED, 102)?;
        let native = custody(desktop).await?;
        capture(
            state,
            desktop,
            name,
            "successor-ack",
            json!({"ack":ack,"native":native}),
            &mut stages,
            &mut captures,
        )
        .await?;
        reopen(state, desktop, recovery::RENAMED).await?;
        capture(
            state,
            desktop,
            name,
            "reopened",
            json!({"freshLabel":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        recovery::release(
            first_request,
            Ok((recovery::response(&team, recovery::LABEL, 101, false), None)),
        )?;
        recovery::acknowledge(first_frame, true)?;
        settle(state)
            .await
            .map_err(|e| format!("Initial settlement: {e}"))?;
        edit(desktop, recovery::EDITED).await?;
        click(desktop, "Use description", false).await?;
        form(desktop).await?;
        if recovery::snapshot(&state.peek())["profile"]["blocks"]
            != json!(recovery::blocks(recovery::EDITED))
        {
            return Err("Native original description not captured".into());
        }
        capture(
            state,
            desktop,
            name,
            "healthy",
            json!({"nativeCaptured":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let event = recovery::changed(&mut state.write(), recovery::RENAMED, 102)?;
        let changed = recovery::snapshot(&state.peek());
        if event["before"]["source"] != event["after"]["source"]
            || changed["source"]["updated"] != 101
            || changed["native"]["profile"]["display_name"] != recovery::RENAMED
        {
            return Err("Changed native event was adopted as canonical source".into());
        }
        capture(
            state,
            desktop,
            name,
            "changed-native",
            event,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        waiting(desktop).await?;
        let (request, frame) = pending(state, 101).await?;
        let opening = recovery::snapshot(&state.peek())["opening"].clone();
        if opening["reading"] != true || opening["mounted"] != false {
            return Err("Missing original waiting opening".into());
        }
        capture(
            state,
            desktop,
            name,
            "opening-held",
            json!({"opening":opening,"request":request,"frame":frame}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let before_http = recovery::snapshot(&state.peek())["automatic"]["http_finished"]
            .as_u64()
            .unwrap();
        recovery::release(
            request,
            Ok((
                recovery::response(&team, recovery::RENAMED, 102, false),
                None,
            )),
        )?;
        http_finished(state, before_http).await?;
        waiting(desktop).await?;
        if recovery::snapshot(&state.peek())["opening"] != opening {
            return Err("Waiting opening changed before ACK".into());
        }
        capture(
            state,
            desktop,
            name,
            "read-before-ack",
            json!({"opening":opening,"request":request,"frame":frame}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let ack = recovery::acknowledge(frame, true)?;
        settle(state)
            .await
            .map_err(|e| format!("Opening settlement: {e}"))?;
        // No click/open call: the owned MissingOpening continuation must mount.
        editor(desktop, recovery::EDITED, recovery::RENAMED).await?;
        let after = recovery::snapshot(&state.peek())["opening"].clone();
        if after["reading"] != false
            || after["mounted"] != true
            || !after["identity"]
                .as_u64()
                .zip(opening["identity"].as_u64())
                .is_some_and(|(next, previous)| next > previous)
            || after["visit"] != opening["visit"]
            || after["serial"] != opening["serial"]
        {
            return Err("Automatic continuation replaced the original opening custody".into());
        }
        source(state, recovery::RENAMED, 102)?;
        capture(
            state,
            desktop,
            name,
            "continued",
            json!({"opening":opening,"afterOpening":after,"ack":ack,"noExtraOpen":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "noLiveNetwork":true,"final":recovery::snapshot(&state.peek()),"visualReview":"required"}),
    )
}
pub(super) async fn waiting(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Loading mention names. Your description is unchanged.'})"#).await.map(|_|())
}
pub(super) async fn successor_pending(
    state: Signal<ShellState>,
    old: u64,
    socket: u64,
) -> Result<(u64, usize), String> {
    for _ in 0..150 {
        let rows = recovery::pending();
        for row in rows.iter().filter(|r| r["kind"] == "discovery") {
            recovery::release(
                row["serial"].as_u64().unwrap(),
                Ok((
                    json!({"ok":true,
                "url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),"ttl_seconds":300}),
                    None,
                )),
            )?;
        }
        let requests: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "member" && r["serial"].as_u64().is_some_and(|s| s > old))
            .collect();
        let frames = recovery::frames()?;
        let frame = frames.iter().find(|f| {
            f["socket"] == socket
                && f["reply"].is_null()
                && f["request"]["subtype"] == "user_change_subscribe_request"
        });
        if requests.len() > 1 {
            return Err("Duplicate successor member acquisition".into());
        }
        if let (Some(request), Some(frame)) = (requests.first(), frame) {
            if request["body"]["updated_ids"] != json!({(recovery::MEMBER):101}) {
                return Err("Successor lost canonical version".into());
            }
            return Ok((
                request["serial"].as_u64().unwrap(),
                frame["index"].as_u64().unwrap() as usize,
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Successor did not acquire under accepted connection: {}",
        recovery::snapshot(&state.peek())
    ))
}
