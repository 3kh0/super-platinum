//! Original physical facts only. No Shell/current owner lookup in an observer.
use super::*;
use serde_json::Value;
use super_platinum_core::slack::transport::{
    ObservedReactionFailure, ReactionAdvice, ReactionRequestKind, ReactionWireEvent,
};

#[derive(Default)]
struct Facts {
    attempted: bool,
    completed: Option<Outcome>,
}
#[derive(Clone)]
pub(crate) struct Trace {
    registry: Registry,
    actor: Actor,
    facts: Arc<Mutex<Facts>>,
}
impl Trace {
    pub(crate) fn new(registry: Registry, actor: Actor) -> Self {
        Self {
            registry,
            actor,
            facts: Arc::default(),
        }
    }
    pub(crate) fn observe(&self, event: ReactionWireEvent) {
        match event {
            ReactionWireEvent::Attempted(ReactionRequestKind::Write) => {
                if let Ok(mut facts) = self.facts.lock() {
                    facts.attempted = true;
                }
            }
            ReactionWireEvent::Response { advice, .. } => match advice {
                ReactionAdvice::None => {}
                ReactionAdvice::Seconds(s) => self.registry.cooldown(&self.actor, Some(s)),
                ReactionAdvice::Permanent => self.registry.permanent(&self.actor),
            },
            ReactionWireEvent::ApiRateLimit(_) => self.registry.cooldown(&self.actor, Some(60)),
            ReactionWireEvent::Completed {
                kind: ReactionRequestKind::Write,
                result,
            } => {
                if let Ok(mut facts) = self.facts.lock() {
                    facts.completed = Some(match result {
                        Ok(raw) => classify(&raw),
                        Err(_) => Outcome::Unknown,
                    });
                }
            }
            // A metadata attempt must never turn an unsent mutation into Unknown.
            _ => {}
        }
    }
    pub(crate) fn attempted(&self) -> bool {
        self.facts.lock().map_or(true, |f| f.attempted)
    }
    pub(crate) fn outcome(&self, result: &Result<Value, ObservedReactionFailure>) -> Outcome {
        let Ok(facts) = self.facts.lock() else {
            return Outcome::Unknown;
        };
        if !facts.attempted {
            return Outcome::NotSent;
        }
        if result.is_err() {
            return Outcome::Unknown;
        }
        // No fabricated successful result may replace missing/ambiguous original
        // completion. Purpose-specific stricter errors can only quarantine more.
        let Some(recorded) = facts.completed else {
            return Outcome::Unknown;
        };
        if recorded == Outcome::Unknown {
            return recorded;
        }
        result.as_ref().map_or(Outcome::Unknown, classify)
    }

    #[cfg(test)]
    pub(crate) fn test_result(
        &self,
        kind: ReactionRequestKind,
        result: &Result<Value, super_platinum_core::slack::Error>,
    ) {
        use super_platinum_core::slack::Error;
        let advice = match result {
            Err(Error::RateLimited { retry_after_secs }) => {
                Some((429, ReactionAdvice::Seconds(retry_after_secs.unwrap_or(60))))
            }
            Err(Error::HttpStatus {
                status,
                retry_after_secs: Some(s),
            }) => Some((*status, ReactionAdvice::Seconds(*s))),
            _ => None,
        };
        if let Some((status, advice)) = advice {
            self.observe(ReactionWireEvent::Response {
                kind,
                status,
                advice,
            });
        }
        if result
            .as_ref()
            .is_ok_and(|raw| raw["ok"] == false && raw["error"] == "ratelimited")
        {
            self.observe(ReactionWireEvent::ApiRateLimit(kind));
        }
        self.observe(ReactionWireEvent::Completed {
            kind,
            result: result.clone(),
        });
    }
}
pub(crate) fn classify(raw: &Value) -> Outcome {
    if raw.get("ok") == Some(&Value::Bool(true)) && raw.get("error").is_none_or(Value::is_null) {
        return Outcome::Acknowledged;
    }
    if raw.get("ok") == Some(&Value::Bool(false))
        && raw
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|code| {
                matches!(
                    code,
                    "already_reacted"
                        | "no_reaction"
                        | "invalid_name"
                        | "message_not_found"
                        | "channel_not_found"
                        | "not_authed"
                        | "invalid_auth"
                        | "account_inactive"
                        | "token_revoked"
                        | "missing_scope"
                        | "is_archived"
                        | "too_many_reactions"
                        | "too_many_emoji"
                        | "invalid_arguments"
                        | "thread_locked"
                        | "access_denied"
                        | "no_permission"
                        | "no_access"
                        | "token_expired"
                        | "bad_timestamp"
                )
            })
    {
        Outcome::Rejected
    } else {
        Outcome::Unknown
    }
}
