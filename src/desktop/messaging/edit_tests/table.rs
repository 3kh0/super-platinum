use super::*;
use crate::resource_reads::render_test;
use dioxus::prelude::*;
use std::{future::Future, pin::Pin, task::Poll, time::Duration};
use super_platinum_core::slack::{
    PreparedRequest, client::RequestBody, models::ThreadViewItem, transport::Health,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn mark(visit: &EditVisit) -> Message {
    Message {
        ts: Some(visit.ts.clone()),
        team: Some(visit.team.clone()),
        channel: Some(visit.channel.clone()),
        text: Some("New supplied table".into()),
        blocks: vec![crate::blocks::table::fixture::block()],
        ..Default::default()
    }
}
fn empty(_: Signal<ShellState>) -> Element {
    rsx! { div {} }
}

#[tokio::test]
async fn known_table_copies_refuse_edit_capture_and_after_link_dispatch_without_losing_draft() {
    // Each canonical copy can veto; other namespaces and retired feed scopes cannot.
    for variant in 0..10 {
        let copy = variant % 5;
        let (mut shell, visit) = captured();
        let mut table = mark(&visit);
        if variant >= 5 {
            table.team = Some("contradictory-embedded-team".into());
            // Feed root.channel itself selects the channel; unlike a channel
            // bag it supplies no independent bucket coordinate to contradict.
            if copy != 2 {
                table.channel = Some("contradictory-embedded-channel".into());
            }
        }
        match copy {
            0 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&visit.team)
                    .unwrap()
                    .messages
                    .get_mut(&visit.channel)
                    .unwrap()
                    .upsert(table);
            }
            1 => {
                shell
                    .core
                    .threads
                    .entry((
                        visit.team.clone(),
                        visit.channel.clone(),
                        "other-root".into(),
                    ))
                    .or_default()
                    .upsert(table);
            }
            _ => {
                shell
                    .core
                    .threads_view
                    .ensure_scope(shell.core.account_epoch, &visit.team);
                let mut item = ThreadViewItem {
                    root_msg: Message {
                        channel: Some(visit.channel.clone()),
                        ts: Some("1810000099.000001".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                match copy {
                    2 => item.root_msg = table,
                    3 => item.unread_replies.push(table),
                    _ => item.latest_replies.push(table),
                }
                shell.core.threads_view.items.push(item);
            }
        }
        let generation = shell.edit_generation;
        let draft = shell.core.edit_composer.text.clone();
        let editing = shell.core.editing.clone();
        assert!(edit_guard::known_table(
            &shell,
            &visit.team,
            &visit.channel,
            &visit.ts
        ));
        assert!(!visit.may_send(&shell));
        assert!(EditVisit::capture(&shell, shell.core.transport.as_ref().unwrap()).is_none());
        shell.start_edit(
            visit.channel.clone(),
            visit.ts.clone(),
            "must not replace".into(),
        );
        assert_eq!(shell.edit_generation, generation);
        assert_eq!(shell.core.edit_composer.text, draft);
        assert_eq!(shell.core.editing, editing);
    }
    let (mut shell, visit) = captured();
    shell
        .core
        .threads
        .entry(("other-team".into(), visit.channel.clone(), visit.ts.clone()))
        .or_default()
        .upsert(mark(&visit));
    shell
        .core
        .threads
        .entry((visit.team.clone(), "other-channel".into(), visit.ts.clone()))
        .or_default()
        .upsert(mark(&visit));
    shell
        .core
        .threads_view
        .ensure_scope(shell.core.account_epoch + 1, &visit.team);
    shell.core.threads_view.items.push(ThreadViewItem {
        root_msg: mark(&visit),
        ..Default::default()
    });
    assert!(visit.may_send(&shell));
    assert!(!edit_guard::known_table(
        &shell,
        &visit.team,
        &visit.channel,
        &visit.ts
    ));

    let (_dom, mut state, _) = render_test::mount("block-kit-static-table", empty);
    let (prepared, visit) = captured();
    let transport = prepared.core.transport.clone().unwrap();
    state.set(prepared);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request = || PreparedRequest {
        method: "POST",
        url: format!("http://{}/api/chat.update", listener.local_addr().unwrap()),
        headers: vec![],
        body: RequestBody::Json(
            serde_json::json!({"channel":visit.channel,"ts":visit.ts,"text":"Submitted edit"}),
        ),
    };
    assert!(!request().retry_safe());
    transport.set_health(Health::Offline);
    let mut pending = Box::pin(execute_edit(state, &visit, &transport, request()));
    std::future::poll_fn(|cx| {
        assert!(
            Future::poll(Pin::as_mut(&mut pending), cx).is_pending(),
            "actual request waits for link"
        );
        Poll::Ready(())
    })
    .await;
    {
        let mut s = state.write();
        s.core
            .workspaces
            .get_mut(&visit.team)
            .unwrap()
            .messages
            .get_mut(&visit.channel)
            .unwrap()
            .upsert(mark(&visit));
    }
    transport.set_health(Health::Unknown);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(state.peek().core.edit_composer.text, " Submitted edit ");
    assert_eq!(state.peek().edit_generation, visit.generation);
    assert_eq!(
        state.peek().core.editing.as_ref(),
        Some(&(visit.channel.clone(), visit.ts.clone()))
    );
    // A result already issued before table arrival is still drained, but cannot
    // replace the newer table body. Do not couple outcome ownership to may_send.
    assert!(apply_edit_result(
        &mut state.write(),
        &visit,
        Ok(receipt(&visit))
    ));
    assert_eq!(text(&state.peek(), &visit), "New supplied table");
    assert!(state.peek().core.editing.is_none());

    // Positive ordinary dispatch through the same helper and one local reply.
    let (prepared, visit) = captured();
    let transport = prepared.core.transport.clone().unwrap();
    state.set(prepared);
    let request = PreparedRequest {
        method: "POST",
        url: format!("http://{}/api/chat.update", listener.local_addr().unwrap()),
        headers: vec![],
        body: RequestBody::Json(
            serde_json::json!({"channel":visit.channel,"ts":visit.ts,"text":"Submitted edit"}),
        ),
    };
    let body=serde_json::to_string(&serde_json::json!({"ok":true,"channel":visit.channel,"ts":visit.ts,"message":{"text":"Submitted edit"}})).unwrap();
    let server = async {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut chunk = [0; 1024];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0 && bytes.len() + count < 16384);
            bytes.extend_from_slice(&chunk[..count]);
        }
        assert!(String::from_utf8_lossy(&bytes).starts_with("POST /api/chat.update HTTP/1.1"));
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    };
    let (result, ()) = tokio::join!(execute_edit(state, &visit, &transport, request), server);
    let sent = result.unwrap().unwrap();
    assert_eq!(sent.channel, visit.channel);
    assert_eq!(sent.ts, visit.ts);
    assert_eq!(sent.message.text.as_deref(), Some("Submitted edit"));
}
