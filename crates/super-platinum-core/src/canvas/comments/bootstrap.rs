//! The Qpc conversations.replies request can create a root. It is a separately
//! journaled one-attempt operation, not an ordinary retry-safe comment read.
use super::*;
use crate::canvas::edit::annotation::{AnnotationRecord, VerifiedAnnotation};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapRecord {
    annotation: AnnotationRecord,
    canonical_control: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_anchor: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    canonical_changed_controls: Vec<Vec<u8>>,
    latest: String,
}
impl BootstrapRecord {
    pub fn annotation(&self) -> &AnnotationRecord {
        &self.annotation
    }
    pub fn canonical_control(&self) -> &[u8] {
        &self.canonical_control
    }
    pub(in crate::canvas) fn canonical_anchor(&self) -> Option<&[u8]> {
        self.canonical_anchor.as_deref()
    }
    pub(in crate::canvas) fn canonical_changed_controls(&self) -> &[Vec<u8>] {
        &self.canonical_changed_controls
    }
    pub fn latest(&self) -> &str {
        &self.latest
    }
    /// Retained MVSX and rGEL both call LYxw.snakeCaseDeep before form
    /// serialization. The nested richText JSON is a string and is not rewritten.
    pub fn wire_snippet(&self) -> Value {
        wire_snippet(self.annotation.snippet())
    }

    pub fn qpc(&self) -> String {
        // Retained rOSe.condenseIdForUrl replaces only the leading temp prefix
        // for the modern IDs admitted by AnnotationRecord.
        format!(
            "Qpc:{}",
            self.annotation.control_id().replacen("temp:", "t:", 1)
        )
    }
    /// Decode inert historical evidence for display. This does not reconstruct
    /// BootstrapAccepted or any admission/settlement authority.
    pub fn observed_root(&self, raw: &Value) -> Result<Message, Invalid> {
        self.validate()?;
        let msg = message(raw, &channel(&self.annotation.scope().file)?)?;
        if !root(&msg)
            || msg
                .extra
                .get("document_comment")
                .and_then(|v| v.get("thread_id"))
                .and_then(Value::as_str)
                != Some(self.annotation.control_id())
        {
            return Err(Invalid::Identity);
        }
        Ok(msg)
    }
    pub fn validate(&self) -> Result<(), Invalid> {
        self.annotation.validate().map_err(|_| Invalid::Identity)?;
        self.annotation
            .check_changed_controls(&self.canonical_changed_controls, false)
            .map_err(|_| Invalid::Identity)?;
        let control = crate::canvas::wire::Section::decode(&self.canonical_control)
            .map_err(|_| Invalid::Message)?;
        self.annotation
            .check_control(&control)
            .map_err(|_| Invalid::Message)?;
        match (
            self.annotation.has_parent_update(),
            self.canonical_anchor.as_deref(),
        ) {
            (false, None) => {}
            (true, Some(raw)) => self
                .annotation
                .check_anchor(
                    &crate::canvas::wire::Section::decode(raw).map_err(|_| Invalid::Message)?,
                    false,
                )
                .map_err(|_| Invalid::Message)?,
            _ => return Err(Invalid::Identity),
        }
        if control.id() != self.annotation.control_id()
            || control
                .sequence()
                .map_err(|_| Invalid::Message)?
                .is_none_or(|n| n == 0)
            || control.section_type().map_err(|_| Invalid::Message)? != Some(9)
            || !valid_latest(&self.latest)
        {
            return Err(Invalid::Identity);
        }
        Ok(())
    }
}
fn wire_snippet(value: &Value) -> Value {
    match value {
        Value::Array(rows) => Value::Array(rows.iter().map(wire_snippet).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let key = match key.as_str() {
                        "richText" => "rich_text",
                        "sectionUsers" => "section_users",
                        "isArchived" => "is_archived",
                        "createdUsec" => "created_usec",
                        "lastEditedUsec" => "last_edited_usec",
                        "controlId" => "control_id",
                        "slackUserId" => "slack_user_id",
                        "slackTeamId" => "slack_team_id",
                        "quipUserId" => "quip_user_id",
                        "isAuthor" => "is_author",
                        "isRecentEditor" => "is_recent_editor",
                        "isMostRecentEditor" => "is_most_recent_editor",
                        "isMentioned" => "is_mentioned",
                        other => other,
                    };
                    (key.to_owned(), wire_snippet(value))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}
pub struct BootstrapIntent {
    record: BootstrapRecord,
}
pub struct BootstrapAccepted {
    record: BootstrapRecord,
    root: Message,
    page: ReplyPage,
}
pub fn valid_latest(value: &str) -> bool {
    let (seconds, fraction) = value
        .split_once('.')
        .map_or((value, None), |(s, f)| (s, Some(f)));
    !seconds.is_empty()
        && seconds.len() <= 13
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && !seconds.starts_with('0')
        && fraction.is_none_or(|f| {
            !f.is_empty()
                && f.len() <= 3
                && f.bytes().all(|b| b.is_ascii_digit())
                && !f.ends_with('0')
        })
}
impl BootstrapIntent {
    pub(in crate::canvas) fn resume_confirmed(
        record: AnnotationRecord,
        canonical_control: Vec<u8>,
        canonical_anchor: Option<Vec<u8>>,
        fresh: &crate::slack::api::CanvasCollabSnapshot,
        now_millis: u64,
    ) -> Result<Self, Invalid> {
        Self::resume_confirmed_with_controls(
            record,
            canonical_control,
            canonical_anchor,
            vec![],
            fresh,
            now_millis,
        )
    }
    pub(in crate::canvas) fn resume_confirmed_with_controls(
        record: AnnotationRecord,
        canonical_control: Vec<u8>,
        canonical_anchor: Option<Vec<u8>>,
        canonical_changed_controls: Vec<Vec<u8>>,
        fresh: &crate::slack::api::CanvasCollabSnapshot,
        now_millis: u64,
    ) -> Result<Self, Invalid> {
        record
            .bootstrap_current_with_controls(
                fresh,
                &canonical_control,
                canonical_anchor.as_deref(),
                &canonical_changed_controls,
            )
            .map_err(|_| Invalid::Identity)?;
        if now_millis < 1000 || now_millis > 9_007_199_254_740_991 {
            return Err(Invalid::Identity);
        }
        let latest = if now_millis % 1000 == 0 {
            (now_millis / 1000).to_string()
        } else {
            format!("{}.{:03}", now_millis / 1000, now_millis % 1000)
                .trim_end_matches('0')
                .into()
        };
        let record = BootstrapRecord {
            annotation: record,
            canonical_control,
            canonical_anchor,
            canonical_changed_controls,
            latest,
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn capture(proof: VerifiedAnnotation, now_millis: u64) -> Result<Self, Invalid> {
        if now_millis < 1000 || now_millis > 9_007_199_254_740_991 {
            return Err(Invalid::Identity);
        }
        let latest = if now_millis % 1000 == 0 {
            (now_millis / 1000).to_string()
        } else {
            format!("{}.{:03}", now_millis / 1000, now_millis % 1000)
                .trim_end_matches('0')
                .into()
        };
        let record = BootstrapRecord {
            annotation: proof.record().map_err(|_| Invalid::Identity)?,
            canonical_control: proof.section().raw().raw().to_vec(),
            canonical_anchor: proof.anchor().map(|s| s.raw().raw().to_vec()),
            canonical_changed_controls: proof.changed_controls().to_vec(),
            latest,
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn record(&self) -> &BootstrapRecord {
        &self.record
    }
    pub fn accept(self, raw: &Value) -> Result<BootstrapAccepted, Invalid> {
        let file = &self.record.annotation.scope().file;
        let channel = channel(file)?;
        let (rows, _) = envelope(raw)?;
        let mut roots = Vec::new();
        for value in rows {
            let msg = message(value, &channel)?;
            if root(&msg) {
                roots.push(msg);
            }
        }
        let [root] = roots.as_slice() else {
            return Err(Invalid::Message);
        };
        if root
            .extra
            .get("document_comment")
            .and_then(|v| v.get("thread_id"))
            .and_then(Value::as_str)
            != Some(self.record.annotation.control_id())
        {
            return Err(Invalid::Identity);
        }
        let page = ReplyPage::parse(file, root.ts.as_deref().ok_or(Invalid::Message)?, true, raw)?;
        Ok(BootstrapAccepted {
            record: self.record,
            root: root.clone(),
            page,
        })
    }
}
impl BootstrapAccepted {
    pub fn record(&self) -> &BootstrapRecord {
        &self.record
    }
    pub fn root(&self) -> &Message {
        &self.root
    }
    pub fn into_page(self) -> ReplyPage {
        self.page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_casing_changes_only_outer_structured_keys_not_rich_json_content() {
        let rich = r#"{"blocks":[{"type":"rich_text","elements":[{"type":"text","text":"sectionUsers"}]}]}"#;
        let original = serde_json::json!({"richText":rich,"mrkdwn":"sectionUsers","isArchived":false,
            "controlId":"temp:x","sectionUsers":[{"quipUserId":"Q1","slackUserId":"U1","isAuthor":true,
                "isRecentEditor":false,"isMostRecentEditor":true,"isMentioned":false}],"createdUsec":1001});
        let actual = wire_snippet(&original);
        assert_eq!(
            actual,
            serde_json::json!({"rich_text":rich,"mrkdwn":"sectionUsers","is_archived":false,
            "control_id":"temp:x","section_users":[{"quip_user_id":"Q1","slack_user_id":"U1","is_author":true,
                "is_recent_editor":false,"is_most_recent_editor":true,"is_mentioned":false}],"created_usec":1001})
        );
        assert_eq!(wire_snippet(&actual), actual);
        assert_eq!(actual["rich_text"], rich);
    }
    #[test]
    fn latest_is_source_millisecond_seconds_not_a_timestamp_or_arbitrary_query() {
        for good in [
            "1700000000",
            "1700000000.001",
            "1700000000.01",
            "1700000000.1",
        ] {
            assert!(valid_latest(good));
        }
        for bad in [
            "",
            "0",
            "01",
            "1.000",
            "1.1234",
            "1e9",
            "NaN",
            "-1",
            "1&token=x",
        ] {
            assert!(!valid_latest(bad));
        }
    }
}
