use super::*;

/// Strict native generic-API acknowledgment, independently of host message
/// correlation. Retained apiCall uses truthy ok; malformed nonboolean values
/// remain an explicit native refusal, not evidence for a different pane gate.
pub fn acknowledged(raw: &Value) -> bool {
    raw["ok"] == true && raw.get("error").is_none_or(Value::is_null)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Acknowledged,
    Rejected,
    Unknown,
    Retired,
    RateLimited,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub number: u8,
    pub status: Option<u16>,
    pub outcome: Outcome,
    pub raw: Option<Value>,
    pub response_text: Option<String>,
    pub retry_after_millis: Option<u64>,
    pub may_have_applied: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Correlation {
    Acknowledged,
    Rejected,
    Unknown,
}
#[derive(Clone)]
pub struct Receipt {
    raw: Value,
    correlation: Correlation,
}
impl Receipt {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn correlation(&self) -> Correlation {
        self.correlation
    }
    pub fn ts(&self) -> Option<&str> {
        (self.correlation == Correlation::Acknowledged)
            .then(|| self.raw["ts"].as_str())
            .flatten()
    }
    pub fn error(&self) -> Option<&str> {
        self.raw.get("error").and_then(Value::as_str)
    }
}
impl Message {
    /// Native host correlation, NOT the retained Share pane's closure gate.
    /// The pane consumes message-queue failures; it does not parse this schema.
    /// No receipt manufactures an optimistic row or a new retry authority.
    pub fn classify(&self, raw: Value) -> Receipt {
        let correlated = acknowledged(&raw)
            && raw["channel"] == self.channel
            && raw["ts"]
                .as_str()
                .is_some_and(crate::slack_lists::comments::timestamp)
            && raw
                .get("client_msg_id")
                .is_none_or(|v| v == &self.client_msg_id)
            && raw.get("message").is_none_or(|m| {
                m.is_object()
                    && m.get("channel").is_none_or(|v| v == &self.channel)
                    && m.get("ts").is_none_or(|v| v == &raw["ts"])
                    && m.get("client_msg_id")
                        .is_none_or(|v| v == &self.client_msg_id)
                    && m.get("thread_ts")
                        .is_none_or(|v| v.is_null() || v == &raw["ts"])
            });
        let rejected = raw["ok"] == false
            && raw["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty() && e != "internal_error");
        Receipt {
            raw,
            correlation: if correlated {
                Correlation::Acknowledged
            } else if rejected {
                Correlation::Rejected
            } else {
                Correlation::Unknown
            },
        }
    }
}
