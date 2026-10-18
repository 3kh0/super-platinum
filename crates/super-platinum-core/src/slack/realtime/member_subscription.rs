//! Member subscription completion belongs to the original socket, not its queue.
//! Retained iPXF/Kot3 contract: subscription_update, incrementing id, reply_to,
//! exact ok:false rejection and socket_closed retirement. The caller owns its
//! operation deadline, reconnect policy and subsequent member-source adoption.
use super::Connection;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::oneshot;

const MAX_SAFE_ID: u64 = 9_007_199_254_740_991;
static MESSAGE_ID: AtomicU64 = AtomicU64::new(16_384);

pub(super) fn is_reply(frame: &Value) -> bool {
    match frame.get("reply_to") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

// Object-key lookup in Kot3 treats a numeric ID and its canonical decimal
// string alike. Refuse other shapes rather than coercing arbitrary JSON.
pub(super) fn reply_id(value: &Value) -> Option<u64> {
    let id = if let Some(id) = value.as_u64() {
        id
    } else if let Some(text) = value.as_str() {
        let id = text.parse::<u64>().ok()?;
        if id.to_string() != text {
            return None;
        }
        id
    } else if let Some(number) = value.as_f64() {
        if !number.is_finite()
            || number.fract() != 0.0
            || !(1.0..=MAX_SAFE_ID as f64).contains(&number)
        {
            return None;
        }
        number as u64
    } else {
        return None;
    };
    (id > 0 && id <= MAX_SAFE_ID).then_some(id)
}

pub(super) fn next_message_id() -> Option<u64> {
    MESSAGE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
            (id < MAX_SAFE_ID).then_some(id + 1)
        })
        .ok()
        .map(|id| id + 1)
}

type Reply = Result<Value, MemberSubscriptionError>;
#[derive(Default)]
struct State {
    closed: bool,
    pending: BTreeMap<u64, oneshot::Sender<Reply>>,
}
#[derive(Default)]
pub(super) struct PendingReplies(Mutex<State>);
impl fmt::Debug for PendingReplies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PendingMemberSubscriptionReplies")
    }
}
impl PendingReplies {
    pub(super) fn close_on_drop(self: &Arc<Self>) -> CloseOnDrop {
        CloseOnDrop(self.clone())
    }
    fn close(&self) {
        let pending = {
            let mut state = self.0.lock().expect("subscription state");
            state.closed = true;
            std::mem::take(&mut state.pending)
        };
        for (_, sender) in pending {
            let _ = sender.send(Err(MemberSubscriptionError::SocketClosed));
        }
    }
    pub(super) fn receive(&self, value: &Value) -> bool {
        let Some(id) = value.get("reply_to").and_then(reply_id) else {
            return false;
        };
        let sender = self
            .0
            .lock()
            .expect("subscription state")
            .pending
            .remove(&id);
        if let Some(sender) = sender {
            let _ = sender.send(Ok(value.clone()));
            true
        } else {
            false
        }
    }
}
pub(super) struct CloseOnDrop(Arc<PendingReplies>);
impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Original request and matching, unmodified server reply; neither grants
/// permission to publish data into a replacement account or connection.
#[derive(Clone, PartialEq, Eq)]
pub struct SubscriptionReceipt {
    request: Value,
    reply: Value,
}
impl SubscriptionReceipt {
    pub fn request(&self) -> &Value {
        &self.request
    }
    pub fn reply(&self) -> &Value {
        &self.reply
    }
    pub fn gateway_error_code(&self) -> Option<i64> {
        self.reply["error"]["code"].as_i64()
    }
    pub fn reason(&self) -> &str {
        self.reply["error"]
            .as_str()
            .or_else(|| self.reply["error"]["msg"].as_str())
            .unwrap_or("unknown_error")
    }
}
impl fmt::Debug for SubscriptionReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubscriptionReceipt")
            .field("id", &self.request["id"])
            .field("gateway_error_code", &self.gateway_error_code())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberSubscriptionError {
    InvalidRequest,
    QueueFull,
    SocketClosed,
    Rejected(SubscriptionReceipt),
}

pub struct MemberSubscription {
    request: Value,
    registry: Weak<PendingReplies>,
    receiver: Option<oneshot::Receiver<Reply>>,
}
impl MemberSubscription {
    pub fn request(&self) -> &Value {
        &self.request
    }
    /// No timeout or retry is hidden here. Dropping this future retires only its
    /// exact local waiter; it does not assert remote unsubscribe or cancellation.
    pub async fn wait(mut self) -> Result<SubscriptionReceipt, MemberSubscriptionError> {
        let reply = self
            .receiver
            .take()
            .expect("one subscription waiter")
            .await
            .map_err(|_| MemberSubscriptionError::SocketClosed)??;
        let receipt = SubscriptionReceipt {
            request: self.request.clone(),
            reply,
        };
        if receipt.reply["ok"] == Value::Bool(false) {
            Err(MemberSubscriptionError::Rejected(receipt))
        } else {
            Ok(receipt)
        }
    }
}
impl Drop for MemberSubscription {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry
                .0
                .lock()
                .expect("subscription state")
                .pending
                .remove(&self.request["id"].as_u64().expect("generated id"));
        }
    }
}

impl Connection {
    /// Closed iPXF member operation, at most 100 supported distinct IDs.
    /// Successful return means locally queued; only wait() supplies completion.
    pub fn request_member_subscription(
        &self,
        ids: &[String],
        subscribe: bool,
    ) -> Result<MemberSubscription, MemberSubscriptionError> {
        crate::block_actions::directory_endpoint::member_info::Read::capture(ids.to_vec())
            .map_err(|_| MemberSubscriptionError::InvalidRequest)?;
        let id = next_message_id().ok_or(MemberSubscriptionError::InvalidRequest)?;
        let subtype = if subscribe {
            "user_change_subscribe_request"
        } else {
            "user_change_unsubscribe_request"
        };
        let request = json!({"type":"subscription_update","subtype":subtype,"ids":ids,"id":id});
        let encoded = request.to_string();
        // Kot3's bound is JS string.length (UTF-16 units), not UTF-8 bytes.
        if encoded.encode_utf16().count() > 32_768 {
            return Err(MemberSubscriptionError::InvalidRequest);
        }
        let (sender, receiver) = oneshot::channel();
        let mut state = self.subscriptions.0.lock().expect("subscription state");
        if state.closed {
            return Err(MemberSubscriptionError::SocketClosed);
        }
        state.pending.insert(id, sender);
        if let Err(error) = self.tx.try_send(encoded) {
            state.pending.remove(&id);
            return Err(match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    MemberSubscriptionError::QueueFull
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    MemberSubscriptionError::SocketClosed
                }
            });
        }
        Ok(MemberSubscription {
            request,
            registry: Arc::downgrade(&self.subscriptions),
            receiver: Some(receiver),
        })
    }

    /// Supply a frame received on THIS connection. Also used by closed offline
    /// fixtures through their real Connection. Unknown IDs cannot settle a read.
    pub fn receive_member_subscription_reply(&self, value: &Value) -> bool {
        self.subscriptions.receive(value)
    }
}

#[cfg(test)]
mod tests;
