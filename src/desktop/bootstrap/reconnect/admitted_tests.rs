//! Isolated-process coverage of the existing closed reconnect reply adapter.
//! No WebView, transport, HTTP, global parent-environment mutation or live Slack.
use super::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    rc::Rc,
    task::Poll,
    time::Duration,
};

const CHILD: &str = "SUPER_PLATINUM_RECONNECT_LOGIC_CHILD";
const NAME: &str = "bootstrap::reconnect::admitted_tests::admitted_read_across_navigation_retains_drafts_and_requires_current_retry";
const RECEIPT: &str = "reconnect-admitted-logic: passed; no session, transport, HTTP or WebView";

#[tokio::test]
async fn admitted_read_across_navigation_retains_drafts_and_requires_current_retry() {
    if std::env::var(CHILD).as_deref() == Ok("1") {
        tokio::time::timeout(Duration::from_secs(10), run())
            .await
            .expect("closed reconnect logic test timed out");
        println!("{RECEIPT}");
        return;
    }
    // The native adapter intentionally requires exact process-local flags.
    // Give it a single-test child instead of racing env::set_var against the
    // parallel suite or adding a production currentness bypass.
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", NAME, "--nocapture"]);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("SUPER_PLATINUM_") {
            command.env_remove(name);
        }
    }
    command
        .env(CHILD, "1")
        .env("SUPER_PLATINUM_FIXTURE", "reconnect-thread-continuation")
        .env("SUPER_PLATINUM_RECONNECT_THREAD_CHECK", "1");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW; no native window.
    }
    let output = command.output().expect("start isolated reconnect test");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains(RECEIPT), "child did not execute: {stdout}");
    assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
}

fn custody(shell: &ShellState, other: &str) -> Value {
    let a = ("T1".into(), fixture::CHANNEL.into(), fixture::ROOT.into());
    let b = ("T1".into(), other.into(), fixture::OTHER.into());
    json!({"channel":shell.core.active_channel,"root":shell.thread_root,
        "scope":shell.thread_scope(),"target":shell.thread_target,
        "loading":shell.thread_loading,"error":shell.thread_error,
        "input":shell.thread_window.input_generation,"atBottom":shell.thread_at_bottom,
        "draft":shell.core.thread_composer.text,
        "attachments":shell.core.thread_composer_attachments.iter().map(|a|(&a.name,a.id)).collect::<Vec<_>>(),
        "cacheA":shell.core.threads[&a].messages,"cacheB":shell.core.threads[&b].messages})
}

fn attachment(id: u64, name: &str) -> super_platinum_core::domain::ComposerAttachment {
    super_platinum_core::domain::ComposerAttachment {
        id,
        name: name.into(),
        path: format!("/unused-offline-fixture/{name}").into(),
        bytes: 12,
        uploading: false,
        upload_started: None,
        upload_cancel: None,
        upload_progress: None,
        preview_path: None,
    }
}

async fn current_read(state: Signal<ShellState>) {
    let (scope, target) = {
        let s = state.peek();
        (s.thread_scope(), s.thread_target.clone())
    };
    super::super::thread::load_started_thread_owned(
        state,
        fixture::CHANNEL.into(),
        fixture::ROOT.into(),
        target,
        scope,
        None,
    )
    .await;
}

async fn run() {
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::apply(&mut shell);
    assert!(fixture::enabled(&shell));
    let channel_a = shell.active_channel;
    let channel_b = shell.channels.iter().position(|c| c.id == "C1").unwrap();
    assert_ne!(channel_a, channel_b);
    let other = shell.channels[channel_b].id.clone();
    let source = ("T1".into(), fixture::CHANNEL.into(), fixture::OTHER.into());
    let bag = shell.core.threads[&source].clone();
    shell
        .core
        .threads
        .insert(("T1".into(), other.clone(), fixture::OTHER.into()), bag);
    shell.core.thread_composer =
        super_platinum_core::ComposerState::with_text("A exact draft \u{4e2d}\u{1f642}");
    shell.core.thread_composer_attachments = vec![attachment(81, "a.png")];
    let owner = Visit::begin(&mut shell);
    let scope = shell.thread_scope();
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Props| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    fixture::arm("thread");
    let mut read = Box::pin(super::super::thread::load_started_thread_owned(
        state,
        fixture::CHANNEL.into(),
        fixture::ROOT.into(),
        None,
        scope,
        Some(owner.clone()),
    ));
    poll_fn(|cx| {
        assert!(read.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let requests = fixture::requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["phase"], "thread");
    assert_eq!(requests[0]["held"], true);
    assert_eq!(requests[0]["released"], false);
    assert!(requests[0]["accepted"].is_null());
    {
        let mut shell = state.write();
        shell.select_channel(channel_b, crate::state::ChannelOpen::Global);
        shell.begin_thread(fixture::OTHER.into(), Some(fixture::OTHER_REPLY.into()));
        shell.core.thread_composer = super_platinum_core::ComposerState::with_text("B exact draft");
        shell.core.thread_composer_attachments = vec![attachment(82, "b.png")];
        assert!(!owner.current(&shell));
        shell.select_channel(channel_a, crate::state::ChannelOpen::Global);
        shell.begin_thread(fixture::ROOT.into(), Some(fixture::TARGET.into()));
        shell.thread_error = Some("Current A visit owns this error".into());
        shell.thread_window.input_generation += 1;
        assert!(!owner.current(&shell));
    }
    let before = custody(&state.peek(), &other);
    assert_eq!(before["draft"], "A exact draft \u{4e2d}\u{1f642}");
    assert_eq!(
        json!(state.peek().core.thread_composer_attachments),
        json!([attachment(81, "a.png")])
    );
    assert!(
        before["cacheA"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["ts"] != fixture::TARGET)
    );
    assert_eq!(fixture::release().unwrap(), 0);
    read.await;
    assert_eq!(custody(&state.peek(), &other), before);
    assert_eq!(fixture::requests()[0]["accepted"], false);
    fixture::fail_next_thread();
    current_read(state).await;
    let failed = custody(&state.peek(), &other);
    assert_eq!(failed["cacheA"], before["cacheA"]);
    assert_eq!(failed["cacheB"], before["cacheB"]);
    assert_eq!(failed["draft"], before["draft"]);
    assert_eq!(failed["attachments"], before["attachments"]);
    assert_eq!(failed["target"], fixture::TARGET);
    assert_eq!(failed["loading"], false);
    assert_eq!(failed["error"], "Replies could not load. Please try again.");
    for key in ["scope", "channel", "root", "input", "atBottom"] {
        assert_eq!(failed[key], before[key]);
    }
    current_read(state).await;
    let recovered = custody(&state.peek(), &other);
    assert_eq!(recovered["error"], Value::Null);
    assert_eq!(recovered["loading"], false);
    for key in ["scope", "channel", "root", "input", "atBottom"] {
        assert_eq!(recovered[key], before[key]);
    }
    assert_eq!(recovered["draft"], before["draft"]);
    assert_eq!(recovered["attachments"], before["attachments"]);
    assert_eq!(recovered["cacheB"], before["cacheB"]);
    assert_eq!(recovered["input"], before["input"]);
    assert_eq!(recovered["target"], fixture::TARGET);
    let rows = recovered["cacheA"].as_array().unwrap();
    assert_eq!(
        rows.iter().filter(|r| r["ts"] == fixture::TARGET).count(),
        1
    );
    let added = rows.iter().find(|r| r["ts"] == fixture::TARGET).unwrap();
    assert_eq!(added["text"], "Recovered exact reply");
    assert_eq!(added["thread_ts"], fixture::ROOT);
    for original in before["cacheA"].as_array().unwrap() {
        assert!(rows.contains(original));
    }
    current_read(state).await;
    assert_eq!(custody(&state.peek(), &other), recovered);
    {
        let mut shell = state.write();
        shell.select_channel(channel_b, crate::state::ChannelOpen::Global);
        shell.begin_thread(fixture::OTHER.into(), Some(fixture::OTHER_REPLY.into()));
        assert_eq!(shell.core.thread_composer.text, "B exact draft");
        assert_eq!(
            json!(shell.core.thread_composer_attachments),
            json!([attachment(82, "b.png")])
        );
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    }
    let requests = fixture::requests();
    assert_eq!(requests.len(), 4);
    for request in &requests {
        assert_eq!(request["released"], true);
        assert!(request["accepted"].is_boolean());
    }
    assert_eq!(requests[1]["outcome"], "error");
    for request in &requests[1..] {
        assert_eq!(request["accepted"], true);
    }
    for request in &requests[2..] {
        assert_eq!(request["requestedTarget"], fixture::TARGET);
        assert_eq!(request["outcome"], "success");
    }
}
