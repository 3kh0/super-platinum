//! Durable custody for an ordinary existing-file Share, separate from forwarding.
//! No function in this module executes, retries, resolves, or overrides a write.
pub mod store;
pub use store::drafts;

use crate::slack::{Error, api::file_share::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub team: String,
    pub user: String,
    pub file: String,
    pub destination: String,
}

/// Credentials and source URLs are deliberately absent. No payload Debug.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    file: String,
    destination: String,
    note: Option<Vec<Value>>,
    context_team: Option<String>,
}
impl Intent {
    pub fn new(
        file: String,
        destination: String,
        note: Option<Vec<Value>>,
        context_team: Option<String>,
    ) -> Result<Self, FileShareInputError> {
        let value = Self {
            file,
            destination,
            note,
            context_team,
        };
        value.request()?;
        Ok(value)
    }
    /// Reconstruction is validation, not permission to dispatch. Only a newly
    /// committed Begin::Started plus the caller's current admission permits it.
    pub fn request(&self) -> Result<ExistingFileShare, FileShareInputError> {
        let note = self.note.clone().map(FileShareNote::new).transpose()?;
        let request = ExistingFileShare::new(self.file.clone(), self.destination.clone(), note)?;
        self.context_team
            .as_ref()
            .map_or(Ok(request.clone()), |team| {
                request.with_context_team(team.clone())
            })
    }
    pub fn note(&self) -> Option<&[Value]> {
        self.note.as_deref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotSent {
    AdmissionCanceled,
    Offline,
}

/// No Debug: the response can contain private server-supplied text.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum Outcome {
    NotSent(NotSent),
    Response(Value),
    ResponseLimit,
    TransportUnknown,
    RateLimited { retry_after_secs: Option<u64> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Includes a crash between journal commit and actual dispatch.
    Unconfirmed,
    NotSent(NotSent),
    Server(FileShareDisposition),
    RateLimited {
        retry_after_secs: Option<u64>,
    },
}

/// Local ID is not a Slack idempotency key. No Debug for note/raw payloads.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub attempt_id: String,
    pub scope: Scope,
    pub intent: Intent,
    pub file_title: String,
    pub destination_name: String,
    pub created_at: u64,
    pub updated_at: u64,
    outcome: Option<Outcome>,
}
impl Receipt {
    pub fn state(&self) -> State {
        match &self.outcome {
            None | Some(Outcome::TransportUnknown) => State::Unconfirmed,
            Some(Outcome::NotSent(reason)) => State::NotSent(*reason),
            Some(Outcome::Response(raw)) => State::Server(
                decode_existing_file_share(raw.clone())
                    .disposition()
                    .clone(),
            ),
            Some(Outcome::ResponseLimit) => State::Server(FileShareDisposition::Uncertain(
                FileShareUncertainty::ResponseLimit,
            )),
            Some(Outcome::RateLimited { retry_after_secs }) => State::RateLimited {
                retry_after_secs: *retry_after_secs,
            },
        }
    }
    pub fn unresolved(&self) -> bool {
        !matches!(
            self.state(),
            State::NotSent(_) | State::Server(FileShareDisposition::Accepted { .. })
        )
    }
    pub fn raw_response(&self) -> Option<&Value> {
        match &self.outcome {
            Some(Outcome::Response(raw)) => Some(raw),
            _ => None,
        }
    }
}

fn classify(result: Result<Value, Error>) -> Outcome {
    match result {
        Ok(raw) => {
            let response = decode_existing_file_share(raw);
            response
                .raw()
                .cloned()
                .map(Outcome::Response)
                .unwrap_or(Outcome::ResponseLimit)
        }
        Err(Error::Api(code)) if code == "draft_mutation_not_sent_offline" => {
            Outcome::NotSent(NotSent::Offline)
        }
        Err(Error::RateLimited { retry_after_secs }) => Outcome::RateLimited { retry_after_secs },
        // Never retain Error strings: they may contain a signed URL or payload.
        Err(_) => Outcome::TransportUnknown,
    }
}

#[cfg(test)]
mod tests;
