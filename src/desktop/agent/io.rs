//! Bounded socket I/O, independent of the native renderer. Dispatch stays serial.
use std::time::Duration;
use super_platinum_core::agent_protocol::{AgentRequest, AgentResponse};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

const CLIENT_DEADLINE: Duration = Duration::from_secs(5);

pub(super) async fn handle<S, F>(stream: S, dispatch: F) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(AgentRequest) -> AgentResponse,
{
    handle_with_deadline(stream, dispatch, CLIENT_DEADLINE).await
}

async fn handle_with_deadline<S, F>(
    stream: S,
    mut dispatch: F,
    deadline: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(AgentRequest) -> AgentResponse,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    loop {
        let line = tokio::time::timeout(deadline, lines.next_line())
            .await
            .map_err(|_| "read: client idle timeout".to_owned())?
            .map_err(|error| format!("read: {error}"))?;
        let Some(line) = line else { return Ok(()) };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<AgentRequest>(&line) {
            Ok(request) => dispatch(request),
            Err(error) => AgentResponse::err(0, format!("invalid request json: {error}")),
        };
        let mut payload = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        payload.push(b'\n');
        tokio::time::timeout(deadline, writer.write_all(&payload))
            .await
            .map_err(|_| "write: client response timeout".to_owned())?
            .map_err(|error| format!("write: {error}"))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn idle_and_partial_line_clients_release_serial_server_without_dispatch() {
        for partial in [b"".as_slice(), b"{\"id\":1".as_slice()] {
            let (mut client, server) = tokio::io::duplex(64);
            client.write_all(partial).await.unwrap();
            let result = handle_with_deadline(
                server,
                |_| panic!("no complete command"),
                Duration::from_millis(20),
            )
            .await;
            assert_eq!(result.unwrap_err(), "read: client idle timeout");
            let mut byte = [0];
            assert_eq!(
                tokio::io::AsyncReadExt::read(&mut client, &mut byte)
                    .await
                    .unwrap(),
                0
            );
        }
    }
    #[tokio::test]
    async fn nonreading_client_times_out_after_one_dispatch_without_replay() {
        let (mut client, server) = tokio::io::duplex(64);
        client
            .write_all(b"{\"id\":1,\"cmd\":\"ping\"}\n")
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let result = handle_with_deadline(
            server,
            move |request| {
                count.fetch_add(1, Ordering::SeqCst);
                AgentResponse::ok(request.id, json!({"large":"x".repeat(4096)}))
            },
            Duration::from_millis(20),
        )
        .await;
        assert_eq!(result.unwrap_err(), "write: client response timeout");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn valid_multi_request_connection_remains_serial_and_closes_cleanly() {
        let (client, server) = tokio::io::duplex(256);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let task = tokio::spawn(async move {
            handle_with_deadline(
                server,
                move |request| {
                    let sequence = count.fetch_add(1, Ordering::SeqCst) + 1;
                    AgentResponse::ok(request.id, json!({"sequence":sequence}))
                },
                Duration::from_secs(1),
            )
            .await
        });
        let (reader, mut writer) = tokio::io::split(client);
        let mut lines = BufReader::new(reader).lines();
        for id in [11, 12] {
            writer
                .write_all(format!("{{\"id\":{id},\"cmd\":\"ping\"}}\n").as_bytes())
                .await
                .unwrap();
            let response: serde_json::Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(response["id"], id);
            assert_eq!(response["ok"], true);
            assert_eq!(response["data"]["sequence"], id - 10);
        }
        writer.shutdown().await.unwrap();
        assert!(task.await.unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
