//! Pure preparation. Supplying argument blocks is not conversion/admission proof.
use super::{
    catalog::{Catalog, Choice, Command},
    *,
};
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frozen {
    pub owner: Owner,
    pub revision: u64,
    pub original: Value,
    pub display: String,
    pub argument_blocks: Option<Value>,
    pub command: Command,
}
impl Frozen {
    pub fn capture(
        owner: Owner,
        revision: u64,
        original: Value,
        display: String,
        argument_blocks: Option<Value>,
        choice: &Choice,
        current: &Catalog,
    ) -> Result<Self, Failure> {
        let command = choice.admit(current, &owner)?.clone();
        let result = Self {
            owner,
            revision,
            original,
            display,
            argument_blocks,
            command,
        };
        result.validate()?;
        Ok(result)
    }
    pub(super) fn validate(&self) -> Result<(), Failure> {
        self.owner.validate()?;
        if self.revision == 0
            || !self.original.is_object()
            || !bounded(&self.original, 256 * 1024)
            || catalog::parse(&self.command.raw).as_ref() != Some(&self.command)
            || !self.command.autocomplete
            || !self.display.eq_ignore_ascii_case(&self.command.name)
            || !catalog::name(&self.display)
            || !catalog::name(&self.command.name)
            || !id(&self.command.app, 'A')
            || self.command.teams.as_slice() != [self.owner.team.clone()]
            || self.owner.thread.is_some() && !self.command.thread_allowed
        {
            return Err(Failure::Unsupported);
        }
        if let Some(blocks) = &self.argument_blocks {
            if !bounded(blocks, 128 * 1024)
                || !blocks.as_array().is_some_and(|rows| {
                    !rows.is_empty()
                        && rows.len() <= 50
                        && rows.iter().all(|row| {
                            row.get("type").and_then(Value::as_str) == Some("rich_text")
                                && row.get("elements").is_some_and(Value::is_array)
                        })
                })
            {
                return Err(Failure::Unsupported);
            }
        }
        Ok(())
    }
}
pub fn discovery(client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        session,
        "client.appCommands",
        vec![("_x_reason", "app-commands-conditional-fetching".into())],
    )
}
pub fn prepare(
    client: &SlackClient,
    session: &WorkspaceSession,
    frozen: &Frozen,
    token: &str,
) -> Result<PreparedRequest, Failure> {
    frozen.validate()?;
    if session.team_id != frozen.owner.team
        || session.user_id != frozen.owner.user
        || token.is_empty()
        || token.len() > 256
        || token.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(Failure::Stale);
    }
    let mut fields = vec![
        ("command", frozen.command.name.clone()),
        ("disp", frozen.display.clone()),
        ("channel", frozen.owner.channel.clone()),
        ("client_token", token.into()),
        ("app_id", frozen.command.app.clone()),
        ("team_id", frozen.owner.team.clone()),
        ("_x_reason", "executeCommand".into()),
    ];
    if let Some(blocks) = &frozen.argument_blocks {
        fields.push(("blocks", blocks.to_string()));
    }
    if let Some(thread) = &frozen.owner.thread {
        fields.push(("thread_ts", thread.clone()));
    }
    Ok(client.rest_form(session, "chat.command", fields))
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Acknowledged {
        keep_input: bool,
        feedback: Option<String>,
        raw: Value,
    },
    Rejected {
        raw: Value,
    },
    Unknown {
        raw: Option<Value>,
    },
}
pub fn decode(raw: Value) -> Reply {
    if !bounded(&raw, 256 * 1024) {
        return Reply::Unknown { raw: None };
    }
    let unknown = || Reply::Unknown {
        raw: Some(raw.clone()),
    };
    if !raw.is_object() {
        return unknown();
    }
    match raw.get("ok").and_then(Value::as_bool) {
        Some(true) if !raw.as_object().unwrap().contains_key("error") => {
            let keep_input = match raw.get("keep_input") {
                None => false,
                Some(v) => match v.as_bool() {
                    Some(v) => v,
                    None => return unknown(),
                },
            };
            let feedback = match raw.get("response") {
                None => None,
                Some(Value::String(v)) => Some(v.clone()),
                Some(_) => return unknown(),
            };
            Reply::Acknowledged {
                keep_input,
                feedback,
                raw,
            }
        }
        Some(false)
            if raw
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty() && v != "dispatch_failed")
                && raw.get("response").is_none()
                && raw.get("keep_input").is_none() =>
        {
            Reply::Rejected { raw }
        }
        _ => unknown(),
    }
}
