use super::*;
use crate::directory_member_fetches::{Context, automatic};
mod continuation;
mod lifecycle;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    let case = recovery::case();
    if matches!(
        case.as_str(),
        "cross-socket-opening" | "opening-failure-cancel"
    ) {
        return continuation::run(state, desktop, name, ready, &case).await;
    }
    if matches!(case.as_str(), "socket-successor" | "changed-opening") {
        return lifecycle::run(state, desktop, name, ready, &case).await;
    }
    if !matches!(case.as_str(), "renamed" | "failure" | "account") {
        return Err("Unknown member freshness case".into());
    }
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
    let (request, frame) = pending(state, 101).await?;
    capture(
        state,
        desktop,
        name,
        "opened-held",
        json!({"request":request,"frame":frame}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "account" {
        let replacement = recovery::replace_account(state);
        let reply = recovery::response("TINIT0001", recovery::RENAMED, 102, false);
        let delivered = recovery::release(request, Ok((reply, None))).is_ok();
        let ack = recovery::acknowledge(frame, true)?;
        wait_dom(desktop, "({ok:!document.querySelector('.profile-edit')})").await?;
        let after = recovery::snapshot(&state.peek());
        if after["native"] != Value::Null || after["sourcePresent"] == true {
            return Err("Retired account adopted automatic member response".into());
        }
        capture(
            state,
            desktop,
            name,
            "account-replaced",
            json!({"replacement":replacement,
            "lateHttpDelivered":delivered,"ack":ack}),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        edit(desktop, recovery::EDITED).await?;
        let remembered = remember(desktop).await?;
        capture(
            state,
            desktop,
            name,
            "native-edited",
            remembered,
            &mut stages,
            &mut captures,
        )
        .await?;
        if case == "renamed" {
            let team = state.peek().core.active_team.clone().unwrap();
            let before_http = recovery::snapshot(&state.peek())["automatic"]["http_finished"]
                .as_u64()
                .ok_or("Missing HTTP completion observation")?;
            recovery::release(
                request,
                Ok((
                    recovery::response(&team, recovery::RENAMED, 102, false),
                    None,
                )),
            )?;
            http_finished(state, before_http).await?;
            // Keep the real ACK pending. Queue acceptance alone grants neither
            // source publication nor replacement of the original native editor.
            let proof = retained(state, desktop, recovery::LABEL, 101).await?;
            capture(
                state,
                desktop,
                name,
                "read-before-ack",
                proof,
                &mut stages,
                &mut captures,
            )
            .await?;
            let ack = recovery::acknowledge(frame, true)?;
            let settled = settle(state).await;
            if settled.is_err() {
                return Err(format!("Rename settlement failed: {settled:?}"));
            }
            source(state, recovery::RENAMED, 102)?;
            let native = custody(desktop).await?;
            capture(
                state,
                desktop,
                name,
                "renamed",
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
            for (index, stage) in ["malformed", "offline", "rejected"].iter().enumerate() {
                let (request, frame) = if index == 0 {
                    (request, frame)
                } else {
                    reenter_stale(state, desktop, recovery::LABEL, 101).await?;
                    pending(state, 101).await?
                };
                let team = state.peek().core.active_team.clone().unwrap();
                let response = match *stage {
                    "malformed" => Ok((json!({"ok":true,"results":"malformed"}), None)),
                    "offline" => Err(Error::Offline("closed automatic member fixture".into())),
                    _ => Ok((
                        recovery::response(&team, recovery::RENAMED, 102, false),
                        None,
                    )),
                };
                recovery::release(request, response)?;
                let ack = recovery::acknowledge(frame, *stage != "rejected")?;
                let result = settle(state).await;
                let native = retained(state, desktop, recovery::LABEL, 101).await?;
                capture(
                    state,
                    desktop,
                    name,
                    stage,
                    json!({"ack":ack,"settlementOk":result.is_ok(),
                    "native":native}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
            }
            reenter_stale(state, desktop, recovery::LABEL, 101).await?;
            let (request, frame) = pending(state, 101).await?;
            let team = state.peek().core.active_team.clone().unwrap();
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
                .map_err(|e| format!("Freshness recovery: {e}"))?;
            source(state, recovery::RENAMED, 102)?;
            editor(desktop, recovery::EDITED, recovery::LABEL).await?;
            let native = custody(desktop).await?;
            capture(
                state,
                desktop,
                name,
                "recovered",
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
        }
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":ready,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "noLiveNetwork":true,"final":recovery::snapshot(&state.peek()),"visualReview":"required"}),
    )
}
async fn settle(state: Signal<ShellState>) -> Result<(), Error> {
    let team = state
        .peek()
        .core
        .active_team
        .clone()
        .ok_or_else(|| Error::Api("fixture_team_missing".into()))?;
    let context = Context::capture(&state.peek(), &team)?;
    automatic::wait(state, &team, &[recovery::MEMBER.into()], |shell| {
        context.current(shell)
    })
    .await?;
    for _ in 0..100 {
        if !context.current(&state.peek()) {
            return Err(Error::Api("fixture_owner_retired".into()));
        }
        if recovery::snapshot(&state.peek())["automatic"]["worker"] == false {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(Error::Api("fixture_worker_not_settled".into()))
}
fn source(state: Signal<ShellState>, name: &str, version: u64) -> Result<(), String> {
    let s = recovery::snapshot(&state.peek());
    if s["source"]["profile"]["display_name"] != name
        || s["source"]["updated"] != version
        || s["native"]["profile"]["display_name"] != name
    {
        return Err(format!("Canonical/native member publication mismatch: {s}"));
    }
    Ok(())
}
async fn retained(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    version: u64,
) -> Result<Value, String> {
    source(state, name, version)?;
    let native = custody(desktop).await?;
    let s = recovery::snapshot(&state.peek());
    if s["profile"]["attempts"] != json!([]) {
        return Err("Unexpected Profile write".into());
    }
    Ok(native)
}
async fn pending(state: Signal<ShellState>, version: u64) -> Result<(u64, usize), String> {
    for _ in 0..150 {
        let rows = recovery::pending();
        for row in rows.iter().filter(|r| r["kind"] == "discovery") {
            recovery::release(
                row["serial"].as_u64().unwrap(),
                Ok((
                    json!({"ok":true,
                "url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),
                "ttl_seconds":300}),
                    None,
                )),
            )?;
        }
        let requests: Vec<_> = rows.iter().filter(|r| r["kind"] == "member").collect();
        if requests.len() > 1 {
            return Err("Duplicate simultaneous member read".into());
        }
        let frames = recovery::frames()?;
        // The actual manager retires a stale subscription before subscribing
        // again. Answer only the retained original unsubscribe request.
        for frame in frames.iter().filter(|f| {
            f["reply"].is_null() && f["request"]["subtype"] == "user_change_unsubscribe_request"
        }) {
            let receipt = recovery::acknowledge(frame["index"].as_u64().unwrap() as usize, true)?;
            if receipt["routed"] != true {
                return Err("Original unsubscribe waiter was retired".into());
            }
        }
        let frames: Vec<_> = frames
            .iter()
            .filter(|f| {
                f["reply"].is_null() && f["request"]["subtype"] == "user_change_subscribe_request"
            })
            .collect();
        if let (Some(request), Some(frame)) = (requests.first(), frames.last()) {
            if request["body"]["updated_ids"] != json!({(recovery::MEMBER):version})
                || request["body"]["check_interaction"] != true
                || request["body"]["include_profile_only_users"] != true
                || request["body"].get("ids").is_some()
            {
                return Err(format!("Wrong conditional member request: {request}"));
            }
            return Ok((
                request["serial"].as_u64().unwrap(),
                frame["index"].as_u64().unwrap() as usize,
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Automatic member request/ACK not queued: {}",
        recovery::snapshot(&state.peek())
    ))
}
async fn form(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
async fn editor(desktop: &DesktopContext, text: &str, label: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);return{{ok:!!v&&e.isContentEditable&&v.snapshot.blocks[0].elements[0].elements[0].text==={}&&v.tokens.length===2&&v.tokens.every(t=>t.label==={})&&v.tokens.map(t=>t.value.fixture_occurrence).join(',')==='1,2'}};}})()"#,json!(text),json!(format!("@{label}")))).await.map(|_|())
}
async fn edit(desktop: &DesktopContext, text: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');if(!e?.isContentEditable||!leaf)return{{ok:false}};leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));const n=leaf.firstChild,r=document.createRange(),s=getSelection();r.setStart(n,2);r.setEnd(n,8);s.removeAllRanges();s.addRange(r);return{{ok:true}};}})()"#,json!(text))).await?;
    editor(desktop, text, recovery::LABEL).await
}
async fn remember(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e),s=getSelection();if(!v)return{ok:false};if(!e.contains(s.anchorNode)||!e.contains(s.focusNode)){const n=e.querySelector('.rich-editor-section > span:not(.rich-editor-token)')?.firstChild;if(!n)return{ok:false};const r=document.createRange();r.setStart(n,2);r.setEnd(n,8);s.removeAllRanges();s.addRange(r);}window.__memberFreshnessEditor={e,pane:e.parentElement,blocks:v.snapshot.blocks,tokens:v.tokens,anchor:s.anchorNode,focus:s.focusNode,start:s.anchorOffset,end:s.focusOffset};return{ok:true,blocks:v.snapshot.blocks,tokens:v.tokens,start:s.anchorOffset,end:s.focusOffset};})()"#).await
}
async fn custody(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const old=window.__memberFreshnessEditor,e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e),s=getSelection();return{ok:!!old&&e===old.e&&e.parentElement===old.pane&&e.isContentEditable&&JSON.stringify(v?.snapshot.blocks)===JSON.stringify(old.blocks)&&JSON.stringify(v?.tokens)===JSON.stringify(old.tokens)&&s.anchorNode===old.anchor&&s.focusNode===old.focus&&s.anchorOffset===old.start&&s.focusOffset===old.end&&!document.getElementById('status-toast'),blocks:v?.snapshot.blocks,tokens:v?.tokens,sameNode:e===old?.e,sameSelection:!!old&&s.anchorNode===old.anchor&&s.focusNode===old.focus&&s.anchorOffset===old.start&&s.focusOffset===old.end,noToast:!document.getElementById('status-toast')};})()"#).await
}
async fn reopen(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    label: &str,
) -> Result<(), String> {
    click(desktop, "Use description", false).await?;
    form(desktop).await?;
    if recovery::snapshot(&state.peek())["profile"]["blocks"]
        != json!(recovery::blocks(recovery::EDITED))
    {
        return Err("Native draft or opaque duplicate token changed on capture".into());
    }
    let count = recovery::requests().len();
    let frame_count = recovery::frames()?.len();
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::EDITED, label).await?;
    if recovery::requests().len() != count || recovery::frames()?.len() != frame_count {
        return Err("Fresh reopening replayed member acquisition".into());
    }
    Ok(())
}
async fn reenter_stale(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    label: &str,
    version: u64,
) -> Result<(), String> {
    let current = wait_dom(
        desktop,
        "({ok:true,editor:!!document.getElementById('profile-section-rich')})",
    )
    .await?;
    click(
        desktop,
        if current["editor"] == true {
            "Use description"
        } else {
            "Cancel description edit"
        },
        false,
    )
    .await?;
    form(desktop).await?;
    if recovery::snapshot(&state.peek())["profile"]["blocks"]
        != json!(recovery::blocks(recovery::EDITED))
    {
        return Err("Original committed description changed between automatic attempts".into());
    }
    let event = recovery::changed(&mut state.write(), label, version)?;
    if event["before"]["source"] != event["after"]["source"] {
        return Err("Socket notification fabricated a canonical member source".into());
    }
    click(desktop, "Edit description", false).await?;
    editor(desktop, recovery::EDITED, label).await?;
    remember(desktop).await?;
    Ok(())
}
async fn http_finished(state: Signal<ShellState>, before: u64) -> Result<(), String> {
    for _ in 0..100 {
        let s = recovery::snapshot(&state.peek());
        if s["automatic"]["http_finished"]
            .as_u64()
            .is_some_and(|n| n > before)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Conditional HTTP did not finish while the real socket ACK was held".into())
}
