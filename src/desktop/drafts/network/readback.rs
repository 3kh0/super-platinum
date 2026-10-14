//! One bounded read-only check. Transport may retry this safe history request;
//! this path never prepares, dispatches, or replays the original write.
use super::*;
use super_platinum_core::drafts::sync::OperationKind;

#[derive(Clone)]
pub(crate) struct Readback {
    pub outcome: Option<Outcome>,
    pub raw: Option<Value>,
    pub issue: Option<Issue>,
    pub cooldown_until_ms: Option<u64>,
    pub requested: bool,
    /// More history may exist. Neither a partial nor a complete page proves
    /// rejection merely because the original UUID was not found.
    pub has_more: bool,
}
impl Readback {
    pub(crate) fn blocked(block: Block) -> Self {
        Self {
            outcome: None,
            raw: None,
            issue: Some(Issue::Blocked(block)),
            cooldown_until_ms: match block {
                Block::Cooldown { until_ms } => Some(until_ms),
                _ => None,
            },
            requested: false,
            has_more: true,
        }
    }
}
impl Binding {
    pub async fn check_operation(&self, operation: &Operation, now_ms: u64) -> Readback {
        match operation.kind {
            OperationKind::Send => self.check_send(operation, now_ms).await,
            OperationKind::Update => self.check_update(operation, now_ms).await,
            _ => Readback::blocked(Block::Unsupported),
        }
    }
    pub async fn check_send(&self, operation: &Operation, now_ms: u64) -> Readback {
        self.check_send_with(operation, now_ms, |request| self.transport.execute(request))
            .await
    }
    pub(super) async fn check_send_with<F, Fut>(
        &self,
        operation: &Operation,
        now_ms: u64,
        read: F,
    ) -> Readback
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let block = if operation.kind != OperationKind::Send {
            Some(Block::Unsupported)
        } else {
            self.prepare(operation, now_ms).err()
        };
        if let Some(block) = block {
            return Readback {
                outcome: None,
                raw: None,
                issue: Some(Issue::Blocked(block)),
                cooldown_until_ms: match block {
                    Block::Cooldown { until_ms } => Some(until_ms),
                    _ => None,
                },
                requested: false,
                has_more: true,
            };
        }
        let request = if let Some(root) = &operation.scope.key.thread {
            api::conversations_replies(
                &self.client,
                &self.workspace,
                api::RepliesArgs {
                    channel: operation.scope.key.channel.clone(),
                    ts: root.clone(),
                    limit: Some(100),
                    ..Default::default()
                },
            )
        } else {
            api::conversations_history(
                &self.client,
                &self.workspace,
                api::HistoryArgs {
                    channel: operation.scope.key.channel.clone(),
                    limit: Some(100),
                    ..Default::default()
                },
            )
        };
        let started = Instant::now();
        let result = read(request).await;
        let received_ms =
            now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        let result = match result {
            Ok(value) if value.get("ok").and_then(Value::as_bool) == Some(false) => {
                let error = Error::Api(
                    value
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown_error")
                        .into(),
                );
                (Some(value), Some(error))
            }
            Ok(value) => return decode_readback(operation, &self.workspace, value),
            Err(error) => (None, Some(error)),
        };
        let failure = failure(&result.1.unwrap(), received_ms);
        if let Some(until) = failure.cooldown_until_ms {
            self.defer_until(until);
        }
        Readback {
            outcome: None,
            raw: result.0,
            issue: Some(if failure.cooldown_until_ms.is_some() {
                Issue::RateLimited
            } else {
                Issue::Uncertain
            }),
            cooldown_until_ms: failure.cooldown_until_ms,
            requested: true,
            has_more: true,
        }
    }
}

pub(super) fn decode_readback(
    operation: &Operation,
    workspace: &WorkspaceSession,
    value: Value,
) -> Readback {
    let has_more = value
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        || value
            .pointer("/response_metadata/next_cursor")
            .and_then(Value::as_str)
            .is_some_and(|cursor| !cursor.is_empty());
    let messages = value.get("messages").and_then(Value::as_array);
    let mut outcome = None;
    let mut issue = Some(Issue::Uncertain);
    if value.get("ok").and_then(Value::as_bool) == Some(true)
        && let Some(messages) = messages
    {
        let matches: Vec<_> = messages
            .iter()
            .filter_map(|message| matching_timestamp(operation, workspace, message))
            .take(2)
            .collect();
        if let [ts] = matches.as_slice() {
            outcome = Some(Outcome::Sent {
                client_msg_id: operation.client_msg_id.clone(),
                draft_id: operation.base.as_ref().map(|base| base.id.clone()),
                ts: (*ts).to_owned(),
            });
            issue = None;
        }
    } else {
        issue = Some(Issue::MalformedResponse);
    }
    Readback {
        outcome,
        raw: Some(value),
        issue,
        cooldown_until_ms: None,
        requested: true,
        has_more,
    }
}

fn matching_timestamp<'a>(
    operation: &Operation,
    workspace: &WorkspaceSession,
    message: &'a Value,
) -> Option<&'a str> {
    if operation.kind != OperationKind::Send
        || operation.scope.key.team != workspace.team_id
        || message.get("client_msg_id").and_then(Value::as_str)
            != Some(operation.client_msg_id.as_str())
        || message.get("user").and_then(Value::as_str) != Some(workspace.user_id.as_str())
        || message
            .get("channel")
            .is_some_and(|channel| channel.as_str() != Some(operation.scope.key.channel.as_str()))
        || message
            .get("is_deleted")
            .is_some_and(|deleted| deleted != &Value::Bool(false))
        || message
            .get("subtype")
            .and_then(Value::as_str)
            .is_some_and(|subtype| matches!(subtype, "message_deleted" | "tombstone"))
    {
        return None;
    }
    let ts = message.get("ts")?.as_str()?;
    if !super::decode::timestamp(ts) {
        return None;
    }
    let correct_thread = match &operation.scope.key.thread {
        Some(root) => {
            message.get("thread_ts").and_then(Value::as_str) == Some(root.as_str())
                && ts != root.as_str()
        }
        None => message
            .get("thread_ts")
            .is_none_or(|root| root.as_str() == Some(ts)),
    };
    correct_thread.then_some(ts)
}
