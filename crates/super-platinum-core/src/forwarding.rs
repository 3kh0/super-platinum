//! Per-destination forwarding receipts. A lost response is not a rejected write.
use crate::slack::{
    Error,
    api::{ForwardMessage, ForwardedMessage},
    models::Message,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub mod store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Sending,
    Sent(String),
    Failed(String),
    Uncertain(String),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub name: String,
    pub request: ForwardMessage,
    pub status: Status,
    pub started_at: u64,
    retry_at: u64,
    previously_uncertain: bool,
}
impl Delivery {
    /// Retain a Retry-After deadline in memory even if persistence is unavailable.
    pub fn defer_until(&mut self, retry_at: u64) {
        self.retry_at = self.retry_at.max(retry_at);
    }

    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }

    pub fn previously_uncertain(&self) -> bool {
        self.previously_uncertain
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deliveries(pub BTreeMap<String, Delivery>);
impl Deliveries {
    pub fn excluded(&self, id: &str) -> bool {
        self.0
            .get(id)
            .is_some_and(|d| !matches!(d.status, Status::Failed(_)))
    }
    pub fn uncertain(&self) -> bool {
        self.0
            .values()
            .any(|d| matches!(d.status, Status::Uncertain(_)))
    }
    /// Normal submission may retry only a definite rejection, after Retry-After.
    pub fn prepare(
        &mut self,
        source: &str,
        ts: &str,
        id: &str,
        name: &str,
        blocks: &[Value],
        now: u64,
    ) -> Option<ForwardMessage> {
        if self.excluded(id) || self.0.get(id).is_some_and(|d| now < d.retry_at) {
            return None;
        }
        let request = self
            .0
            .get(id)
            .filter(|d| d.request.blocks == blocks)
            .map(|d| d.request.clone())
            .unwrap_or_else(|| {
                ForwardMessage::new(source.into(), ts.into(), id.into(), blocks.into())
            });
        self.0.insert(
            id.into(),
            Delivery {
                name: name.into(),
                request: request.clone(),
                status: Status::Sending,
                started_at: now,
                retry_at: 0,
                previously_uncertain: false,
            },
        );
        Some(request)
    }
    /// Caller must obtain explicit confirmation. The original note and ID survive.
    pub fn resend(&mut self, id: &str, now: u64) -> Option<ForwardMessage> {
        let d = self.0.get_mut(id)?;
        if !matches!(d.status, Status::Uncertain(_)) || now < d.retry_at {
            return None;
        }
        d.previously_uncertain = true;
        d.status = Status::Sending;
        Some(d.request.clone())
    }
    pub fn finish(
        &mut self,
        request: &ForwardMessage,
        result: Result<ForwardedMessage, Error>,
        now: u64,
    ) -> bool {
        let Some(d) = self.0.get_mut(&request.destination) else {
            return false;
        };
        if d.request.client_msg_id != request.client_msg_id || d.status != Status::Sending {
            return false;
        }
        d.status = match result {
            Ok(receipt) if receipt.channel == request.destination && valid_ts(&receipt.ts) => {
                Status::Sent(receipt.ts)
            }
            Ok(_) => Status::Uncertain(
                "Slack returned an unexpected receipt. Check delivery before resending.".into(),
            ),
            Err(error) => {
                if let Error::RateLimited { retry_after_secs } = &error {
                    d.retry_at = d
                        .retry_at
                        .max(now.saturating_add(retry_after_secs.unwrap_or(30)));
                }
                if definite_rejection(&error) && !d.previously_uncertain {
                    Status::Failed(failure_message(&error))
                } else {
                    Status::Uncertain(
                        "Delivery could not be confirmed. Check before resending.".into(),
                    )
                }
            }
        };
        true
    }
    /// Positive history evidence only. Absence, inaccessible history and text matches
    /// cannot prove delivery or authorize an automatic resend.
    pub fn confirm(
        &mut self,
        id: &str,
        request_id: &str,
        sender: &str,
        messages: &[Message],
    ) -> bool {
        let Some(d) = self.0.get_mut(id) else {
            return false;
        };
        if !matches!(d.status, Status::Uncertain(_)) || d.request.client_msg_id != request_id {
            return false;
        }
        let found = messages.iter().find(|m| {
            m.client_msg_id.as_deref() == Some(request_id)
                && m.user.as_deref() == Some(sender)
                && m.ts.as_deref().is_some_and(valid_ts)
        });
        if let Some(ts) = found.and_then(|m| m.ts.clone()) {
            d.status = Status::Sent(ts);
            true
        } else {
            false
        }
    }
}
fn valid_ts(ts: &str) -> bool {
    ts.split_once('.').is_some_and(|(s, f)| {
        !s.is_empty() && f.len() == 6 && s.bytes().chain(f.bytes()).all(|b| b.is_ascii_digit())
    })
}
fn definite_rejection(error: &Error) -> bool {
    match error {
        Error::TransportNotConfigured | Error::RateLimited { .. } => true,
        Error::Api(code) => matches!(
            code.as_str(),
            "channel_not_found"
                | "not_in_channel"
                | "is_archived"
                | "restricted_action"
                | "not_allowed"
                | "permission_denied"
                | "no_permission"
                | "invalid_auth"
                | "not_authed"
                | "token_revoked"
                | "account_inactive"
                | "message_not_found"
                | "invalid_arguments"
                | "invalid_blocks"
        ),
        // Offline includes timeouts and connection resets after the body was sent.
        _ => false,
    }
}
fn failure_message(error: &Error) -> String {
    match error {
        Error::RateLimited {
            retry_after_secs: Some(seconds),
        } => format!("Slack asked you to wait {seconds} seconds before retrying."),
        Error::RateLimited { .. } => {
            "Slack is limiting requests. Wait 30 seconds before retrying.".into()
        }
        Error::TransportNotConfigured => "Reconnect to Slack to forward this message.".into(),
        Error::Api(code)
            if matches!(
                code.as_str(),
                "channel_not_found" | "not_in_channel" | "is_archived"
            ) =>
        {
            "This conversation is unavailable for forwarding.".into()
        }
        Error::Api(code)
            if matches!(
                code.as_str(),
                "restricted_action" | "not_allowed" | "permission_denied" | "no_permission"
            ) =>
        {
            "You do not have permission to forward here.".into()
        }
        _ => "Slack rejected this forward. Your note is still here.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare(d: &mut Deliveries, id: &str, note: &str) -> ForwardMessage {
        d.prepare(
            "SOURCE",
            "100.000001",
            id,
            id,
            &[Value::String(note.into())],
            100,
        )
        .unwrap()
    }
    fn sent(id: &str) -> Result<ForwardedMessage, Error> {
        Ok(ForwardedMessage {
            channel: id.into(),
            ts: "101.000001".into(),
        })
    }
    #[test]
    fn partial_batch_retries_only_rejected_destinations() {
        let mut d = Deliveries::default();
        let a = prepare(&mut d, "A", "note");
        let b = prepare(&mut d, "B", "note");
        let c = prepare(&mut d, "C", "note");
        d.finish(&a, sent("A"), 101);
        d.finish(&b, Err(Error::Api("not_in_channel".into())), 101);
        d.finish(&c, Err(Error::Offline("timeout".into())), 101);
        assert!(d.excluded("A"));
        assert!(!d.excluded("B"));
        assert!(d.excluded("C"));
        assert!(d.uncertain());
        assert!(
            d.prepare("SOURCE", "100.000001", "A", "A", &[], 102)
                .is_none()
        );
        assert!(
            d.prepare("SOURCE", "100.000001", "C", "C", &[], 102)
                .is_none()
        );
        assert_eq!(prepare(&mut d, "B", "note").client_msg_id, b.client_msg_id);
        d.finish(&b, Err(Error::Api("not_in_channel".into())), 103);
        assert_ne!(
            prepare(&mut d, "B", "edited").client_msg_id,
            b.client_msg_id
        );
    }
    #[test]
    fn explicit_resend_preserves_original_and_prior_uncertainty() {
        let mut d = Deliveries::default();
        let r = prepare(&mut d, "A", "rich note");
        d.finish(&r, Err(Error::Transport("reset".into())), 101);
        let again = d.resend("A", 102).unwrap();
        assert_eq!(again.client_msg_id, r.client_msg_id);
        assert_eq!(again.blocks, r.blocks);
        d.finish(&again, Err(Error::Api("not_in_channel".into())), 103);
        assert!(d.uncertain()); // Rejection of the retry says nothing about the first attempt.
        let again = d.resend("A", 104).unwrap();
        d.finish(&again, sent("A"), 105);
        assert!(d.resend("A", 106).is_none());
    }
    #[test]
    fn receipt_identity_and_rate_limit_are_enforced() {
        let mut d = Deliveries::default();
        let r = prepare(&mut d, "A", "note");
        let mut stale = r.clone();
        stale.client_msg_id = "old".into();
        assert!(!d.finish(&stale, sent("A"), 101));
        d.finish(&r, sent("B"), 101);
        assert!(d.uncertain());
        let r = d.resend("A", 102).unwrap();
        d.finish(
            &r,
            Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }),
            103,
        );
        assert!(d.resend("A", 162).is_none());
        assert!(d.resend("A", 163).is_some());
        let b = prepare(&mut d, "B", "note");
        d.finish(
            &b,
            Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }),
            103,
        );
        assert!(
            d.prepare("SOURCE", "100.000001", "B", "B", &[], 162)
                .is_none()
        );
        assert!(
            d.prepare("SOURCE", "100.000001", "B", "B", &[], 163)
                .is_some()
        );
        let delivery = d.0.get_mut("B").unwrap();
        delivery.defer_until(250);
        delivery.defer_until(200);
        assert_eq!(delivery.retry_at(), 250);
    }
    #[test]
    fn history_needs_current_request_sender_and_valid_timestamp() {
        let mut d = Deliveries::default();
        let r = prepare(&mut d, "A", "note");
        d.finish(&r, Err(Error::Offline("timeout".into())), 101);
        let mut m = Message {
            client_msg_id: Some(r.client_msg_id.clone()),
            user: Some("ME".into()),
            ts: Some("101.000002".into()),
            ..Default::default()
        };
        assert!(!d.confirm("A", &r.client_msg_id, "OTHER", &[m.clone()]));
        assert!(!d.confirm("A", "old", "ME", &[m.clone()]));
        assert!(!d.confirm("A", &r.client_msg_id, "ME", &[]));
        m.ts = Some("bad".into());
        assert!(!d.confirm("A", &r.client_msg_id, "ME", &[m.clone()]));
        m.ts = Some("101.000002".into());
        assert!(d.confirm("A", &r.client_msg_id, "ME", &[m]));
        assert!(!d.uncertain());
        assert!(d.excluded("A"));
    }
}
