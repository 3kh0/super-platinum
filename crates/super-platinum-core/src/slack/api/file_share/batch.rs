//! Ordered existing-file set for the retained composer files.share branch.
//! Wire validation only: never an upload receipt, source permission, or dispatch
//! capability. Runtime integration must retain custody for the ENTIRE file set.
use super::*;

/// Local resource bound, not a claimed Slack product limit.
pub const MAX_EXISTING_FILES: usize = 32;

#[derive(Clone, PartialEq)]
pub struct ExistingFilesShare {
    files: Vec<String>,
    destination: String,
    note: Option<FileShareNote>,
    context_team: Option<String>,
}
impl ExistingFilesShare {
    /// Duplicate IDs are refused, not silently reordered or deduplicated.
    /// Preserving order matters: Slack's draft UI supports explicit reordering.
    pub fn new(
        files: Vec<String>,
        destination: String,
        note: Option<FileShareNote>,
    ) -> Result<Self, FileShareInputError> {
        if files.is_empty()
            || files.len() > MAX_EXISTING_FILES
            || files.iter().any(|f| !id(f, b"F", 2))
            || files
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != files.len()
        {
            return Err(FileShareInputError::FileId);
        }
        if !id(&destination, b"CGD", 9) {
            return Err(FileShareInputError::Destination);
        }
        Ok(Self {
            files,
            destination,
            note,
            context_team: None,
        })
    }
    pub fn with_context_team(mut self, team: String) -> Result<Self, FileShareInputError> {
        if !id(&team, b"T", 9) {
            return Err(FileShareInputError::ContextTeam);
        }
        self.context_team = Some(team);
        Ok(self)
    }
    pub fn file_ids(&self) -> &[String] {
        &self.files
    }
    pub fn destination(&self) -> &str {
        &self.destination
    }
}

/// Complete existing files with optional already-resolved rich blocks. Upload
/// continuation, threads/broadcast, file grants and pending slugs are separate
/// source-proven branches; this type cannot request them accidentally.
pub fn existing_files_share(
    client: &SlackClient,
    session: &WorkspaceSession,
    intent: &ExistingFilesShare,
) -> PreparedRequest {
    let mut fields = vec![
        ("files", intent.files.join(",")),
        ("channel", intent.destination.clone()),
    ];
    if let Some(note) = &intent.note {
        fields.push(("blocks", note.encoded.clone()));
    }
    fields.extend([
        ("resharing_aware", "true".into()),
        ("skip_dlp_user_warning", "false".into()),
        ("from_share_modal", "false".into()),
    ]);
    if let Some(team) = &intent.context_team {
        fields.push(("client_context_team_id", team.clone()));
    }
    client.rest_form(session, "files.share", fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    use serde_json::json;
    fn session() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://synthetic.slack.com".into(),
            token: "synthetic-token".into(),
        }
    }
    fn fields(intent: &ExistingFilesShare) -> Vec<(String, String)> {
        let RequestBody::Form(v) =
            existing_files_share(&SlackClient::default(), &session(), intent).body
        else {
            panic!("expected form")
        };
        v
    }
    #[test]
    fn ordered_batch_keeps_unicode_blocks_and_only_source_proven_ordinary_flags() {
        let blocks = vec![
            json!({"type":"rich_text","future":7,"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"日本語 🚀"}]}]}),
        ];
        let intent = ExistingFilesShare::new(
            vec!["F00000002".into(), "F00000001".into()],
            "D00000001".into(),
            Some(FileShareNote::new(blocks.clone()).unwrap()),
        )
        .unwrap();
        assert_eq!(intent.file_ids(), ["F00000002", "F00000001"]);
        assert_eq!(
            fields(&intent),
            vec![
                ("token".into(), "synthetic-token".into()),
                ("files".into(), "F00000002,F00000001".into()),
                ("channel".into(), "D00000001".into()),
                ("blocks".into(), serde_json::to_string(&blocks).unwrap()),
                ("resharing_aware".into(), "true".into()),
                ("skip_dlp_user_warning".into(), "false".into()),
                ("from_share_modal".into(), "false".into())
            ]
        );
    }
    #[test]
    fn singleton_wire_matches_existing_builder_and_context_is_explicit() {
        let one =
            ExistingFilesShare::new(vec!["F00000001".into()], "C00000001".into(), None).unwrap();
        let old = ExistingFileShare::new("F00000001".into(), "C00000001".into(), None).unwrap();
        let RequestBody::Form(old) =
            existing_file_share(&SlackClient::default(), &session(), &old).body
        else {
            panic!("expected form")
        };
        assert_eq!(fields(&one), old);
        assert_eq!(
            fields(&one.with_context_team("T00000002".into()).unwrap())
                .last()
                .unwrap(),
            &("client_context_team_id".into(), "T00000002".into())
        );
    }
    #[test]
    fn malformed_duplicate_or_oversized_sets_and_person_channels_are_refused() {
        for files in [
            vec![],
            vec!["F1".into(), "F1".into()],
            vec!["F1,F2".into()],
            vec!["F1\n".into()],
            vec!["f1".into()],
            (0..=MAX_EXISTING_FILES).map(|n| format!("F{n}")).collect(),
        ] {
            assert!(ExistingFilesShare::new(files, "C00000001".into(), None).is_err());
        }
        for channel in ["U00000001", "person:U00000001", "C1", "C00000001,D00000001"] {
            assert!(ExistingFilesShare::new(vec!["F1".into()], channel.into(), None).is_err());
        }
    }
}
