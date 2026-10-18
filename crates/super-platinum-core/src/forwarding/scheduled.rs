//! Detached scheduled forwards. Their durable owner is not the active composer.
//! Requests are drafts.create/update, never chat.shareMessage plus a time field.
//! The journal commits ambiguity before yielding a one-use request capability.
use crate::{
    config::WorkspaceSession,
    error::AppError,
    slack::{PreparedRequest, SlackClient},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub mod discovery;
mod request;
pub mod store;
pub use request::{list_match, valid_readback};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub team: String,
    pub user: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub channel: String,
    pub timestamp: String,
    pub thread: Option<String>,
    pub author: Option<String>,
    pub permalink: String,
    pub private: bool,
}

/// The captured forward picker categorizes a selected conversation or member.
/// Scheduling to a member does not create a DM as a prerequisite.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    Conversation(String),
    Person(String),
}
impl Destination {
    pub fn id(&self) -> &str {
        match self {
            Self::Conversation(id) | Self::Person(id) => id,
        }
    }
    pub fn wire(&self) -> Vec<Value> {
        vec![match self {
            Self::Conversation(id) => json!({"channel_id":id}),
            Self::Person(id) => json!({"user_ids":[id]}),
        }]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    owner: Owner,
    source: Source,
    destination: Destination,
    destination_name: String,
    note: Vec<Value>,
    blocks: Vec<Value>,
    post_at: i64,
    client_msg_id: String,
    private_confirmed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Ready,
    Unknown,
    Confirmed,
    Rejected,
    Conflict,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Schedule,
    SaveAsDraft,
    Delete,
}

/// All rich values, identities and prior/observed raw responses survive restart.
/// A read is not admission to retry an ambiguous write.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    id: String,
    revision: u64,
    intent: Intent,
    phase: Phase,
    action: Action,
    attempt: Option<String>,
    response_recorded: bool,
    retry_at: u64,
    updated_at: u64,
    reviewed: Option<Value>,
    observed: Option<Value>,
    outcome: String,
}

/// Only Journal::reserve constructs this. Cloning/listing a Record cannot send.
pub struct Attempt {
    record: Record,
    request_taken: bool,
    client_last_updated_ts: u64,
}

/// An immutable response and spent attempt. Local persistence may borrow this
/// repeatedly; it exposes no request capability and cannot change the outcome.
/// Keep it alive until Journal::finish succeeds, including after owner retirement.
pub struct Completion {
    attempt: Attempt,
    result: Result<Value, crate::slack::Error>,
    completed_at: u64,
}
impl Completion {
    pub fn record(&self) -> &Record {
        &self.attempt.record
    }
}

/// Spent known-unsent capability. Its only use is a storage-only release retry.
pub struct Unsent {
    attempt: Attempt,
    completed_at: u64,
}
impl Unsent {
    pub fn record(&self) -> &Record {
        &self.attempt.record
    }
}

pub(crate) fn invalid(message: &'static str) -> AppError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
impl Source {
    fn validate(&self) -> Result<(), AppError> {
        if !id(&self.channel)
            || !super::valid_ts(&self.timestamp)
            || self.thread.as_deref().is_some_and(|v| !super::valid_ts(v))
            || self.author.as_deref().is_some_and(|v| !id(v))
        {
            return Err(invalid("Invalid scheduled-forward source"));
        }
        let url =
            url::Url::parse(&self.permalink).map_err(|_| invalid("Invalid source permalink"))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid("Invalid source permalink"));
        }
        if url.path()
            != format!(
                "/archives/{}/p{}",
                self.channel,
                self.timestamp.replace('.', "")
            )
            || url.fragment().is_some()
        {
            return Err(invalid("The source permalink names a different message"));
        }
        let query: Vec<_> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let expected = match self.thread.as_ref().filter(|v| *v != &self.timestamp) {
            Some(thread) => vec![
                ("thread_ts".to_owned(), thread.clone()),
                ("cid".to_owned(), self.channel.clone()),
            ],
            None => vec![],
        };
        if query != expected {
            return Err(invalid("The source permalink names a different thread"));
        }
        Ok(())
    }
    fn reference(&self) -> Value {
        let mut atom = json!({"type":"message_mention","message_ts":self.timestamp,"channel_id":self.channel,"text":self.permalink,"url":self.permalink});
        if let Some(author) = &self.author {
            atom["author_id"] = author.clone().into();
        }
        if let Some(thread) = &self.thread {
            atom["thread_ts"] = thread.clone().into();
        }
        atom
    }
}

/// Append the typed source to the final ordinary section without flattening any
/// note leaf/unknown attribute. A final list/quote/preformatted block keeps its
/// boundary and gets a subsequent plain section, as the retained Delta converter.
pub fn append_reference(note: &[Value], source: &Source) -> Result<Vec<Value>, AppError> {
    source.validate()?;
    if note.iter().any(|v| !v.is_object()) || serde_json::to_vec(note)?.len() > 1_048_576 {
        return Err(invalid("The rich note cannot be retained"));
    }
    let mut blocks = note.to_vec();
    let atom = source.reference();
    if let Some(last) = blocks.last_mut().filter(|v| v["type"] == "rich_text") {
        let elements = last
            .get_mut("elements")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("Malformed rich note"))?;
        if let Some(section) = elements
            .last_mut()
            .filter(|v| v["type"] == "rich_text_section")
        {
            section
                .get_mut("elements")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| invalid("Malformed rich section"))?
                .push(atom);
        } else {
            elements.push(json!({"type":"rich_text_section","elements":[atom]}));
        }
    } else {
        blocks.push(
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[atom]}]}),
        );
    }
    Ok(blocks)
}

impl Intent {
    pub fn new(
        owner: Owner,
        source: Source,
        destination: Destination,
        destination_name: String,
        note: Vec<Value>,
        post_at: i64,
        private_confirmed: bool,
        now: i64,
    ) -> Result<Self, AppError> {
        if !id(&owner.team)
            || !id(&owner.user)
            || !id(destination.id())
            || destination_name.is_empty()
            || destination_name.len() > 16_384
        {
            return Err(invalid("Invalid scheduled-forward owner or destination"));
        }
        if source.private && !private_confirmed {
            return Err(invalid("Confirm forwarding this private message first"));
        }
        crate::scheduled::validate_time(post_at, now).map_err(invalid)?;
        let blocks = append_reference(&note, &source)?;
        Ok(Self {
            owner,
            source,
            destination,
            destination_name,
            note,
            blocks,
            post_at,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            private_confirmed,
        })
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn destination(&self) -> &Destination {
        &self.destination
    }
    pub fn destination_name(&self) -> &str {
        &self.destination_name
    }
    pub fn note(&self) -> &[Value] {
        &self.note
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    pub fn post_at(&self) -> i64 {
        self.post_at
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
    fn validate(&self) -> Result<(), AppError> {
        self.source.validate()?;
        if !id(&self.owner.team)
            || !id(&self.owner.user)
            || !id(self.destination.id())
            || uuid::Uuid::parse_str(&self.client_msg_id).is_err()
            || self.post_at < 0
            || (self.source.private && !self.private_confirmed)
            || append_reference(&self.note, &self.source)? != self.blocks
        {
            return Err(invalid("Invalid retained scheduled forward"));
        }
        Ok(())
    }
}

impl Record {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn action(&self) -> Action {
        self.action
    }
    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }
    pub fn outcome(&self) -> &str {
        &self.outcome
    }
    pub fn reviewed(&self) -> Option<&Value> {
        self.reviewed.as_ref()
    }
    pub fn observed(&self) -> Option<&Value> {
        self.observed.as_ref()
    }
    pub fn server_id(&self) -> Option<&str> {
        self.reviewed.as_ref()?.get("draft")?.get("id")?.as_str()
    }
    fn validate(&self) -> Result<(), AppError> {
        self.intent.validate()?;
        if uuid::Uuid::parse_str(&self.id).is_err()
            || self.revision > i64::MAX as u64
            || self
                .attempt
                .as_ref()
                .is_some_and(|v| uuid::Uuid::parse_str(v).is_err())
        {
            return Err(invalid("Invalid retained scheduled-forward record"));
        }
        Ok(())
    }
}
impl Attempt {
    pub fn unsent(self, completed_at: u64) -> Unsent {
        Unsent {
            attempt: self,
            completed_at,
        }
    }
    /// Transfer the original network outcome into storage-only custody.
    pub fn complete(
        self,
        result: Result<Value, crate::slack::Error>,
        completed_at: u64,
    ) -> Completion {
        Completion {
            attempt: self,
            result,
            completed_at,
        }
    }
    pub fn record(&self) -> &Record {
        &self.record
    }
    /// Matching credentials are supplied only at the host's final owner fence.
    /// A capability yields its prepared mutation once, even before any dispatch.
    pub fn take_request(
        &mut self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
    ) -> Result<PreparedRequest, AppError> {
        if self.request_taken
            || workspace.team_id != self.record.intent.owner.team
            || workspace.user_id != self.record.intent.owner.user
        {
            return Err(invalid(
                "Retired or already consumed scheduled-forward attempt",
            ));
        }
        let source = &self.record.intent.source;
        let canonical = crate::slack::api::workspace_permalink(
            &workspace.url,
            &source.channel,
            &source.timestamp,
            source
                .thread
                .as_deref()
                .filter(|thread| *thread != source.timestamp),
        );
        if canonical.as_deref() != Some(source.permalink.as_str()) {
            return Err(invalid(
                "The source belongs to a different workspace origin",
            ));
        }
        let request =
            request::mutation(client, workspace, &self.record, self.client_last_updated_ts)?;
        self.request_taken = true;
        Ok(request)
    }
}
