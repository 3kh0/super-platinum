//! O0Pg/rguE: a private audio file followed by an explicit whole-Header save.
//! A recording is not a generic File editor or a text pronunciation edit.
use super::*;
use std::collections::HashSet;
pub mod custody;

pub const FILENAME: &str = "audio_name_pronunciation.m4a";
pub const SUBTYPE: &str = "slack_name_pronunciation";
pub const MAX_DURATION_MS: u32 = 10_000;
pub const WARNING_MS: u32 = 5_000;
// Local resource bound, not a claimed Slack upload limit.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;

/// Request-bound private data. Deliberately no Debug or serialization.
#[derive(Clone)]
pub struct RecordingDraft {
    baseline: OwnProfileSections,
    section: Value,
    target: String,
    initial: Option<String>,
    desired: Option<String>,
}
impl RecordingDraft {
    pub fn open(snapshot: &OwnProfileSections) -> Result<Self, Error> {
        let (section, target) = target(snapshot)?;
        // Convert now as well as at Save: an unknown sibling is never dropped.
        schema::convert(section)?;
        let file = schema::text(target, "fileId")?;
        if !file.is_empty() && !valid_file(file) {
            return Err(invalid("The existing recording identity is unreadable."));
        }
        let initial = (!file.is_empty()).then(|| file.to_owned());
        Ok(Self {
            baseline: snapshot.clone(),
            section: section.clone(),
            target: required(target, "elementId")?.into(),
            desired: initial.clone(),
            initial,
        })
    }
    pub fn current_file(&self) -> Option<&str> {
        self.desired.as_deref()
    }
    pub fn original_file(&self) -> Option<&str> {
        self.initial.as_deref()
    }
    pub fn dirty(&self) -> bool {
        self.initial != self.desired
    }
    pub fn replace(&mut self, file: &str) -> Result<(), Error> {
        if !valid_file(file) {
            return Err(invalid("The recording file identity is invalid."));
        }
        self.desired = Some(file.into());
        Ok(())
    }
    /// Clearing the profile reference does not delete the underlying shared file.
    pub fn remove(&mut self) {
        self.desired = None;
    }
    pub fn revalidate(&self, fresh: &OwnProfileSections) -> Result<(), Error> {
        let (section, _) = target(fresh)?;
        if fresh.team_id() != self.baseline.team_id()
            || fresh.user_id() != self.baseline.user_id()
            || section != &self.section
        {
            return Err(invalid(
                "Your profile changed. Review the current Header before saving.",
            ));
        }
        Ok(())
    }
    pub fn confirms(&self, fresh: &OwnProfileSections) -> bool {
        if fresh.team_id() != self.baseline.team_id() || fresh.user_id() != self.baseline.user_id()
        {
            return false;
        }
        Self::open(fresh).is_ok_and(|next| {
            next.target == self.target
                && next.section["sectionId"] == self.section["sectionId"]
                && next.initial == self.desired
        })
    }
    pub fn request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
        fresh: &OwnProfileSections,
    ) -> Result<PreparedRequest, Error> {
        self.revalidate(fresh)?;
        if w.team_id != self.baseline.team_id()
            || w.user_id != self.baseline.user_id()
            || !self.dirty()
        {
            return Err(invalid("There is no recording change for this account."));
        }
        let mut elements = schema::convert(&self.section)?;
        let item = elements
            .iter_mut()
            .find(|e| e["element_id"] == self.target)
            .ok_or_else(|| invalid("The recording field is unavailable."))?;
        item["file"] = self
            .desired
            .as_ref()
            .map_or(Value::Null, |file| json!({"fileId":file}));
        Ok(c.rest_form(
            w,
            "users.profile.setSections",
            vec![
                ("user", w.user_id.clone()),
                ("section", required(&self.section, "sectionId")?.into()),
                ("elements", Value::Array(elements).to_string()),
                ("_x_reason", "edit-profile-form-pane".into()),
            ],
        ))
    }
}

fn target(snapshot: &OwnProfileSections) -> Result<(&Value, &Value), Error> {
    // The own-profile read is bound to the current session user by its decoder.
    // The captured Header enables recording from target existence, NOT text's
    // permissions.ui nor custom-section canEdit. Do not invent those gates.
    if snapshot.raw()["result"]["data"]["user"]["isSelf"] != true {
        return Err(invalid("Slack did not identify this as your profile."));
    }
    let mut headers = snapshot.sections().iter().filter(|s| s["type"] == "HEADER");
    let header = headers
        .next()
        .ok_or_else(|| invalid("The profile Header is unavailable."))?;
    if headers.next().is_some() {
        return Err(invalid("The profile Header is ambiguous."));
    }
    let fields = header["profileElements"]
        .as_array()
        .ok_or_else(|| invalid("Header fields are unavailable."))?;
    let mut ids = HashSet::new();
    for field in fields {
        if !ids.insert(required(field, "elementId")?) {
            return Err(invalid("Header field identities are ambiguous."));
        }
    }
    let mut matches = fields
        .iter()
        .filter(|e| e["elementKey"] == "Name Recording");
    let target = matches
        .next()
        .ok_or_else(|| invalid("Name recording is unavailable for this profile."))?;
    if matches.next().is_some() || target["__typename"] != "ProfileFileElement" {
        return Err(invalid("Name recording has an unsupported schema."));
    }
    Ok((header, target))
}
pub fn valid_file(id: &str) -> bool {
    (9..=64).contains(&id.len())
        && id.starts_with('F')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// No file bytes, URLs or credentials in Debug/serde. Upload is a separate
/// one-attempt sequence; obtaining a ticket never changes the profile.
#[derive(Clone)]
pub struct UploadTicket {
    file: String,
    url: String,
}
impl UploadTicket {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        success(raw)?;
        let file = raw["file"]
            .as_str()
            .filter(|s| valid_file(s))
            .ok_or_else(|| invalid("Slack did not identify the recording upload."))?;
        let url = required(raw, "upload_url")?;
        if !valid_upload_url(url) {
            return Err(invalid(
                "Slack returned an invalid recording upload destination.",
            ));
        }
        Ok(Self {
            file: file.into(),
            url: url.into(),
        })
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}
pub(crate) fn valid_upload_url(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.port_or_known_default() == Some(443)
            && u.host_str()
                .is_some_and(|h| h == "files.slack.com" || h.ends_with(".files.slack.com"))
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
    })
}
pub fn ticket_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    length: usize,
) -> Result<PreparedRequest, Error> {
    if length == 0 || length > MAX_BYTES {
        return Err(invalid("The recording is empty or too large."));
    }
    Ok(c.rest_form(
        w,
        "files.getUploadURL",
        vec![
            ("filename", FILENAME.into()),
            ("length", length.to_string()),
            ("subtype", SUBTYPE.into()),
            ("_x_reason", "upload-queue".into()),
        ],
    ))
}
pub fn complete_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    ticket: &UploadTicket,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.completeUpload",
        vec![
            (
                "files",
                json!([{"id":ticket.file,"title":FILENAME,"subtype":SUBTYPE}]).to_string(),
            ),
            ("_x_reason", "upload-queue".into()),
        ],
    )
}
pub fn success(raw: &Value) -> Result<(), Error> {
    if raw["ok"] == false
        && let Some(code) = raw["error"].as_str().filter(|s| !s.is_empty())
    {
        return Err(Error::Api(code.into()));
    }
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(Error::Transport(
            "Slack did not confirm the recording operation.".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests;
