//! Disconnected native clicks through the production Canvas anchor handler.
use super::*;
use crate::slack_links_check::{wait_dom, wait_state};
use serde_json::{Value, json};

pub(super) const HTML: &str = r#"<div class="quip-canvas-content">
<h1>Canvas links</h1>
<p>Keep conversations and people inside the workspace.</p>
<p><a href="slack://channel?team=T1&amp;id=C1">Open general</a></p>
<p><a href="https://echonet.slack.com/archives/C2/p1719700000000100">Open message</a></p>
<p><a href="slack://channel?team=T1&amp;id=C2&amp;message=1719800001.000001&amp;thread_ts=1719800000.000100">Open reply</a></p>
<p><a href="slack://user?team=T1&amp;id=U1">Open Maya</a> · <a href="slack://open?team=T1">Open workspace</a></p>
<p><a href="slack://unsupported?team=T1">Unsupported link</a> · <a href="slack://channel?team=TOTHER&amp;id=C1">Other workspace</a></p>
<p><a href="https://example.com/design">External reference</a> · <a href="mailto:person@example.com">Email</a></p>
</div>"#;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-links" | "canvas-links-narrow")
        )
}

pub(crate) fn install(shell: &mut ShellState) {
    crate::slack_links_check::install(shell);
    show(shell);
}

fn show(shell: &mut ShellState) {
    shell.close_profile();
    shell.close_thread();
    shell.toast = None;
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    shell.canvas = Some(CanvasTarget {
        id: "Ffixture".into(),
        title: "Canvas links".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    checklist::open_visit(shell);
    // An opened visit is deliberately stale until its canonical read finishes.
    // Install a real synthetic snapshot through that same read boundary rather
    // than bypassing Rename's stale/read-ownership guard in the renderer.
    let read = checklist::begin_read(shell).expect("Canvas links fixture read");
    let snapshot = canonical(shell);
    assert!(checklist::finish_read(shell, &read, Ok(snapshot)));
}

fn canonical(shell: &ShellState) -> Arc<api::CanvasCollabSnapshot> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use super_platinum_core::canvas::wire::EncodedField;
    let snapshot = text::fixture::snapshot("Canvas links fixture", 1000);
    let session = &shell.core.session.as_ref().unwrap().workspaces["T1"];
    let options = snapshot
        .init
        .options()
        .replace_fields(
            33,
            &[EncodedField::bytes(33, session.user_id.as_bytes()).unwrap()],
        )
        .unwrap()
        .replace_fields(
            34,
            &[EncodedField::bytes(34, session.team_id.as_bytes()).unwrap()],
        )
        .unwrap();
    let init = api::CanvasControllerInit::parse(
        json!({
            "ok":true,"quip_user_id":snapshot.init.quip_user_id(),
            "init_options":STANDARD.encode(options.raw()),
        }),
        session,
    )
    .unwrap();
    Arc::new(
        api::CanvasCollabSnapshot::from_responses(
            snapshot.file.clone(),
            init,
            snapshot.load.clone(),
        )
        .unwrap(),
    )
}

fn disconnected(shell: &ShellState) -> bool {
    shell.core.transport.is_none()
        && shell.core.session.as_ref().is_some_and(|session| {
            session.d_cookie.is_empty()
                && session.workspaces.len() == 1
                && session.workspaces.get("T1").is_some_and(|workspace| {
                    workspace.token.is_empty() && workspace.url == "https://echonet.slack.com"
                })
        })
}

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested() || std::env::var("SUPER_PLATINUM_CANVAS_LINKS_CHECK").as_deref() != Ok("1") {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(std::time::Duration::from_secs(180), run(state, &desktop))
                .await
            {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => json!({"ok":false,"error":error}),
                Err(_) => json!({"ok":false,"error":"Canvas links timed out"}),
            };
        eprintln!("canvas-links-check: {receipt}");
    });
}

async fn click(desktop: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(desktop, &format!(r#"(()=>{{const a=[...document.querySelectorAll('.canvas-dialog a')].find(a=>a.textContent==={});if(!a)return {{ok:false}};a.scrollIntoView({{block:'nearest'}});a.focus();a.click();return {{ok:true}};}})()"#, json!(label))).await.map(|_| ()).map_err(|error|format!("Click Canvas link {label}: {error}"))
}

async fn reopen(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    show(&mut state.write());
    wait_dom(
        desktop,
        r#"({ok:!!document.querySelector('.canvas-dialog a')})"#,
    )
    .await
    .map(|_| ())
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait_dom(
        desktop,
        r#"({ok:!!document.querySelector('.canvas-dialog a')})"#,
    )
    .await?;
    if !disconnected(&state.peek()) {
        return Err("Expected disconnected Canvas links fixture".into());
    }
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    capture_stage(state, desktop, "initial", &mut captures, &mut geometry).await?;
    let origin = state.peek().core.active_channel.clone();
    for (label, stage) in [
        ("Unsupported link", "unsupported"),
        ("Other workspace", "foreign-workspace"),
    ] {
        click(desktop, label).await?;
        wait_state(state, |shell| {
            shell.toast.is_some() && shell.overlay == Some(Overlay::Canvas)
        })
        .await?;
        if state.peek().core.active_channel != origin {
            return Err("Rejected link changed conversation".into());
        }
        capture_stage(state, desktop, stage, &mut captures, &mut geometry).await?;
        state.write().toast = None;
    }
    wait_dom(desktop, r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Rename');if(!b||b.disabled)return {ok:false};b.click();return {ok:true};})()"#).await.map_err(|error|format!("Open Rename after foreign-workspace check: {error}"))?;
    wait_dom(desktop, r#"(()=>{const e=document.getElementById('canvas-name');if(!e)return {ok:false};e.value='Keep this unsaved title';e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    let before = state.peek().slack_link_generation;
    click(desktop, "Open general").await?;
    wait_state(state, |shell| shell.toast.is_some()).await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('canvas-name')?.value==='Keep this unsaved title'})"#,
    )
    .await?;
    if state.peek().slack_link_generation != before || state.peek().core.active_channel != origin {
        return Err("Editing link started navigation".into());
    }
    wait_dom(desktop, r#"(()=>{const e=document.getElementById('canvas-name');e?.scrollIntoView({block:'nearest'});return {ok:!!e};})()"#).await?;
    capture_stage(
        state,
        desktop,
        "unsaved-rename",
        &mut captures,
        &mut geometry,
    )
    .await?;
    wait_dom(desktop, r#"(()=>{const b=document.querySelector('.canvas-rename button[type=button]');if(!b)return {ok:false};b.click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.canvas-rename')})"#,
    )
    .await?;
    wait_state(state, |shell| surface::can_leave(shell)).await?;
    click(desktop, "Open general").await?;
    wait_state(state, |shell| {
        shell.core.active_channel.as_deref() == Some("C1") && shell.overlay.is_none()
    })
    .await?;
    reopen(state, desktop).await?;
    click(desktop, "Open message").await?;
    wait_state(state, |shell| {
        shell.core.active_channel.as_deref() == Some("C2") && shell.overlay.is_none()
    })
    .await?;
    wait_dom(desktop, r#"(()=>{const p=document.getElementById('message-timeline'),e=p?.querySelector('[data-message-id="1719700000.000100"]'),r=e?.getBoundingClientRect(),c=p?.getBoundingClientRect();return {ok:!!r&&!!c&&r.width>0&&r.bottom>c.top&&r.top<c.bottom};})()"#).await?;
    capture_stage(state, desktop, "message", &mut captures, &mut geometry).await?;
    reopen(state, desktop).await?;
    click(desktop, "Open reply").await?;
    wait_state(state, |shell| {
        shell.thread_root.as_deref() == Some("1719800000.000100") && !shell.thread_at_bottom
    })
    .await?;
    wait_dom(desktop, r#"(()=>{const p=document.getElementById('thread-body'),e=p?.querySelector('[data-message-id="1719800001.000001"]'),r=e?.getBoundingClientRect(),c=p?.getBoundingClientRect();return {ok:!!r&&!!c&&r.width>0&&r.bottom>c.top&&r.top<c.bottom};})()"#).await?;
    capture_stage(state, desktop, "reply", &mut captures, &mut geometry).await?;
    reopen(state, desktop).await?;
    click(desktop, "Open Maya").await?;
    wait_state(state, |shell| {
        shell.core.active_channel.as_deref() == Some("D1")
            && shell.profile_user.as_deref() == Some("U1")
    })
    .await
    .map_err(|error| {
        format!(
            "Open Maya (profile={:?}, overlay={:?}): {error}",
            state.peek().profile_user,
            state.peek().overlay
        )
    })?;
    reopen(state, desktop).await?;
    click(desktop, "Open workspace").await?;
    wait_state(state, |shell| {
        shell.core.active_team.as_deref() == Some("T1") && shell.overlay.is_none()
    })
    .await
    .map_err(|error| {
        format!(
            "Open workspace (team={:?}, overlay={:?}): {error}",
            state.peek().core.active_team,
            state.peek().overlay
        )
    })?;
    reopen(state, desktop).await?;
    let mut receipt = wait_dom(desktop, r#"(()=>{const d=document.querySelector('.canvas-dialog'),p=d?.querySelector('.canvas-scroll');if(p)p.scrollTop=0;return {ok:!!d&&d.scrollWidth<=d.clientWidth+1&&document.documentElement.scrollWidth<=innerWidth+1&&d.querySelector('a[href="https://example.com/design"]')?.textContent==='External reference'&&d.querySelector('a[href="mailto:person@example.com"]')?.textContent==='Email',viewport:[innerWidth,innerHeight]};})()"#).await?;
    if !disconnected(&state.peek()) {
        return Err("Canvas links acquired credentials or transport".into());
    }
    for key in [
        "channelNavigated",
        "messageVisible",
        "replyVisible",
        "userNavigated",
        "workspaceNavigated",
        "rejectedLinksRetainedCanvas",
        "editorDraftPreserved",
        "externalAndMailtoRetained",
        "noTransport",
        "blankCredentials",
    ] {
        receipt[key] = json!(true);
    }
    receipt["preparedRequests"] = json!(0);
    capture_stage(state, desktop, "final", &mut captures, &mut geometry).await?;
    receipt["captures"] = Value::Object(captures);
    receipt["geometry"] = json!(geometry);
    Ok(receipt)
}

/// Save the native window before advancing from the asserted visual state.
async fn capture_stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    if !disconnected(&state.peek()) {
        return Err("Canvas link capture requires blank credentials and no transport".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or("Canvas link capture directory required")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Canvas fixture required")?;
    let destination = directory.join(format!("{fixture}-{stage}.png"));
    let fresh = destination.clone();
    if tokio::task::spawn_blocking(move || fresh.try_exists())
        .await
        .map_err(|_| "Inspect capture destination failed")?
        .map_err(|_| "Inspect capture destination failed")?
    {
        return Err("Canvas link capture destination must be fresh".into());
    }
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let expected_feedback = match stage {
        "unsupported" => Some("This Slack link is not supported yet."),
        "foreign-workspace" => Some("This Slack workspace is not signed in here."),
        "unsaved-rename" => Some("Finish or cancel your canvas edit before opening this link."),
        _ => None,
    };
    let feedback = if let Some(expected) = expected_feedback {
        Some(wait_dom(desktop, &format!(r#"(()=>{{
            const e=document.getElementById('canvas-link-feedback'),r=e?.getBoundingClientRect(),d=e?.closest('.canvas-dialog')?.getBoundingClientRect();
            const hit=(x,y)=>{{const a=document.elementFromPoint(x,y);return a===e||!!e?.contains(a)}};
            const visible=!!r&&!!d&&r.width>0&&r.height>0&&r.left>=Math.max(0,d.left)&&r.top>=Math.max(0,d.top)&&r.right<=Math.min(innerWidth,d.right)+1&&r.bottom<=Math.min(innerHeight,d.bottom)+1&&e.scrollHeight<=e.clientHeight+1&&e.scrollWidth<=e.clientWidth+1&&hit(r.left+1,r.top+1)&&hit(r.right-1,r.bottom-1);
            return {{ok:visible&&e.textContent==={},text:e?.textContent,visible,rect:r?[r.x,r.y,r.width,r.height]:null}};
        }})()"#, json!(expected))).await.map_err(|error|format!("Visible Canvas link feedback at {stage}: {error}"))?)
    } else {
        None
    };
    let mut measured = wait_dom(desktop, r#"(()=>{
        const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return [r.x,r.y,r.width,r.height]};
        const d=document.querySelector('.canvas-dialog'),n=document.getElementById('canvas-name'),m=document.getElementById('message-timeline'),t=document.getElementById('thread-body');
        return {ok:document.documentElement.scrollWidth<=innerWidth+1&&(!d||d.scrollWidth<=d.clientWidth+1),viewport:[innerWidth,innerHeight],canvas:rect(d),rename:rect(n),messageTimeline:rect(m),thread:rect(t)};
    })()"#).await?;
    measured["stage"] = json!(stage);
    measured["feedback"] = json!(feedback);
    eprintln!(
        "canvas-links-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(destination.to_string_lossy().into_owned()), desktop)
            .await?;
    measured["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    geometry.push(measured);
    eprintln!(
        "canvas-links-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reopened_link_fixture_finishes_matching_canonical_read_before_rename() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell);
        let first = checklist::presentation(&shell).unwrap();
        assert!(!first.stale && !first.loading && first.operation.is_none());
        let snapshot = first.snapshot.unwrap();
        assert_eq!(snapshot.file_id(), "Ffixture");
        assert_eq!(
            snapshot.init.options().string(33).unwrap(),
            Some(shell.self_account.user_id.as_str())
        );
        assert_eq!(snapshot.init.workspace_id(), "T1");
        assert_eq!(snapshot.file.extra["editable"], json!(true));
        let mut reader = Reader {
            document: Some(Arc::new(CanvasDocument::parse(HTML).unwrap())),
            editable: true,
            ..Default::default()
        };
        checklist::reader::update_mapping(&shell, &mut reader);
        assert_eq!(reader.checklist.owner, Some((first.visit, first.read)));
        assert!(reader.editable && !reader.loading && !reader.canonical_loading);
        show(&mut shell);
        let reopened = checklist::presentation(&shell).unwrap();
        assert!(!reopened.stale && !reopened.loading && reopened.operation.is_none());
        assert_ne!((reopened.visit, reopened.read), (first.visit, first.read));
        assert!(disconnected(&shell));
    }
    #[test]
    fn canvas_links_fixture_has_known_routes_and_no_credentials_or_transport() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell);
        assert!(disconnected(&shell));
        assert_eq!(shell.overlay, Some(Overlay::Canvas));
        assert!(CanvasDocument::parse(HTML).is_ok());
        assert!(shell.core.threads.contains_key(&(
            "T1".into(),
            "C2".into(),
            "1719800000.000100".into()
        )));
    }
}
