use super::*;
use super_platinum_core::slack::api;
pub(super) async fn run(
    state: Signal<ShellState>,
    attempt: &Attempt,
) -> Result<HistoryPage, Error> {
    let (transport, client, session) = {
        let shell = state.peek();
        if !attempt.action.current(&shell) {
            return Err(Error::Transport("Delivery check owner changed".into()));
        }
        let transport = shell
            .core
            .transport
            .clone()
            .ok_or_else(|| Error::Offline("No active transport".into()))?;
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&attempt.action.key.team))
            .cloned()
            .ok_or_else(|| Error::Offline("No active session".into()))?;
        (transport, shell.core.client.clone(), session)
    };
    let expected = attempt.action.expected();
    let scan = super::super::ownership::upload_pages::find(
        &expected,
        |cursor| {
            let request = if let Some(root) = &expected.thread_ts {
                api::conversations_replies(
                    &client,
                    &session,
                    api::RepliesArgs {
                        channel: expected.channel.clone(),
                        ts: root.clone(),
                        cursor,
                        limit: Some(100),
                        ..Default::default()
                    },
                )
            } else {
                api::conversations_history(
                    &client,
                    &session,
                    api::HistoryArgs {
                        channel: expected.channel.clone(),
                        cursor,
                        limit: Some(100),
                        ..Default::default()
                    },
                )
            };
            let transport = &transport;
            async move {
                if !read_allowed(&state.peek(), attempt, Instant::now()) {
                    return Err(Error::Transport(
                        "Delivery readback is paused or its owner changed".into(),
                    ));
                }
                let value = transport.execute_upload_readback(request).await?;
                if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
                    return Err(Error::Transport(
                        "Delivery readback was not confirmed".into(),
                    ));
                }
                serde_json::from_value(value)
                    .map_err(|_| Error::Transport("Delivery readback could not be decoded".into()))
            }
        },
        || attempt.action.current(&state.peek()),
    );
    tokio::time::timeout(Duration::from_secs(30), scan)
        .await
        .unwrap_or_else(|_| {
            Err(Error::Transport(
                "Delivery readback exceeded its time budget".into(),
            ))
        })
}
