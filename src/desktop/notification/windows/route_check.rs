//! Disconnected synthetic COM invocation through the production activation hook.
//! This proves dispatch/routing, never an actual Windows toast click.
use super::super::route::Destination;
use super::*;
use dioxus::prelude::*;

pub(super) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel")
        && std::env::var("SUPER_PLATINUM_NOTIFICATION_ROUTE_CHECK").as_deref() == Ok("1")
        && std::env::var_os("SUPER_PLATINUM_NOTIFICATION_CHECK").is_none()
        && std::env::var_os("SUPER_PLATINUM_TASKBAR_BADGE_CHECK").is_none()
        && std::env::var_os("SUPER_PLATINUM_NOTIFICATION_ACTIVATION_FIXTURE").is_none()
}

pub(super) async fn run(state: Signal<crate::state::ShellState>) {
    tokio::time::sleep(Duration::from_secs(2)).await;
    match verify(state).await {
        Ok(receipt) => eprintln!("notification-route-check: {receipt}"),
        Err(error) => eprintln!(
            "notification-route-check: {}",
            json!({"ok":false,"error":error})
        ),
    }
}

async fn verify(mut state: Signal<crate::state::ShellState>) -> Result<Value, String> {
    if !enabled()
        || state.peek().core.transport.is_some()
        || state.peek().core.session.is_some()
        || state.peek().core.cache.is_some()
    {
        return Err("Route check requires a disconnected uncached fixture".into());
    }
    let root = "1719800000.000100";
    let message = "1719800001.000001";
    let (first, latest) = {
        let mut shell = state.write();
        crate::slack_links_check::install(&mut shell);
        shell.core.active_account = Some("notification-route-fixture".into());
        (
            Destination::capture(&shell.core, "T1", "C3", None, None)
                .ok_or("Missing first route")?,
            Destination::capture(&shell.core, "T1", "C2", Some(message), Some(root))
                .ok_or("Missing latest route")?,
        )
    };
    let initial = navigation(&state.peek());
    activation::fixture_callback(&first.launch(), true)?;
    activation::fixture_callback(&latest.launch(), false)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let cold_preserved = navigation(&state.peek()) == initial;
    if !cold_preserved {
        return Err("Cold activation navigated before session readiness".into());
    }
    session_ready();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let done = {
            let shell = state.peek();
            shell.core.active_channel.as_deref() == Some("C2")
                && shell.thread_root.as_deref() == Some(root)
                && !shell.thread_at_bottom
        };
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let exact_thread = {
        let shell = state.peek();
        shell.core.active_channel.as_deref() == Some("C2")
            && shell.thread_root.as_deref() == Some(root)
            && !shell.thread_at_bottom
            && shell.thread_messages.iter().any(|row| row.id == message)
            && shell.main_view == crate::state::MainView::Home
    };
    let visible = crate::slack_links_check::wait_dom(&dioxus::desktop::window(), r#"(()=>{const panel=document.getElementById('thread-body'),clip=panel?.getBoundingClientRect();const row=panel?.querySelector('[data-message-id="1719800001.000001"]');const r=row?.getBoundingClientRect();const visible=!!r&&!!clip&&r.width>0&&r.bottom>Math.max(0,clip.top)&&r.top<Math.min(innerHeight,clip.bottom);return {ok:visible&&row.textContent.includes('This is the exact linked reply.'),replyVisible:visible,viewport:[innerWidth,innerHeight]};})()"#).await?;
    let before_preview = navigation(&state.peek());
    activation::fixture_callback("open", false)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let preview_only = navigation(&state.peek()) == before_preview;
    state.write().core.active_account = Some("different-fixture-account".into());
    let before_mismatch = navigation(&state.peek());
    activation::fixture_callback(&first.launch(), false)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let wrong_account = navigation(&state.peek()) == before_mismatch;
    let no_transport = state.peek().core.transport.is_none() && state.peek().core.cache.is_none();
    state.write().core.session = None;
    state.write().core.active_account = None;
    Ok(
        json!({"ok":cold_preserved&&exact_thread&&preview_only&&wrong_account&&no_transport&&visible["replyVisible"]==true,
        "syntheticComCallback":true,"actualOsClick":false,"coldQueuedLatest":cold_preserved&&exact_thread,
        "exactThreadDestination":exact_thread,"wrongAccountPreserved":wrong_account,
        "replyVisible":visible["replyVisible"],"layout":visible,
        "previewForegroundOnly":preview_only,"noTransport":no_transport,"noCache":state.peek().core.cache.is_none(),
        "activations":activation::count(),"destination":navigation(&state.peek())}),
    )
}

fn navigation(shell: &crate::state::ShellState) -> Value {
    json!({"channel":shell.core.active_channel,"root":shell.thread_root,"target":shell.thread_target,
        "channelGeneration":shell.channel_generation,"threadGeneration":shell.thread_generation,
        "linkGeneration":shell.slack_link_generation})
}
