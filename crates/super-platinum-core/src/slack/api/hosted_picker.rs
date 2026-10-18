//! UnifiedFilePicker's ordinary-file browser path. Read-only; Insert is separate.
//! Retained 8zfi -> initializeUnifiedFilePickerBrowsers -> HKCz -> wBUp -> YhiB.
use super::super::{PreparedRequest, client::SlackClient};
use crate::config::WorkspaceSession;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum HostedFileKind {
    #[default]
    All,
    Pdfs,
    Documents,
    Spreadsheets,
    Presentations,
    Images,
}
impl HostedFileKind {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Pdfs,
        Self::Documents,
        Self::Spreadsheets,
        Self::Presentations,
        Self::Images,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All files",
            Self::Pdfs => "PDFs",
            Self::Documents => "Documents",
            Self::Spreadsheets => "Spreadsheets",
            Self::Presentations => "Presentations",
            Self::Images => "Images",
        }
    }
    fn restriction(self) -> &'static str {
        match self {
            Self::All => {
                "type:pdfs type:documents type:spreadsheets type:presentations type:images"
            }
            Self::Pdfs => "type:pdfs",
            Self::Documents => "type:documents",
            Self::Spreadsheets => "type:spreadsheets",
            Self::Presentations => "type:presentations",
            Self::Images => "type:images",
        }
    }
}
pub fn hosted_picker_search(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    text: &str,
    kind: HostedFileKind,
    page: u32,
) -> Result<PreparedRequest, super::super::Error> {
    if text.len() > 4096 || text.chars().any(char::is_control) || page == 0 || page > 1000 {
        return Err(super::super::Error::Api(
            "unsupported_hosted_picker_query".into(),
        ));
    }
    let query = if text.trim().is_empty() {
        kind.restriction().to_owned()
    } else {
        format!("{} {}", text.trim(), kind.restriction())
    };
    Ok(client.rest_form(
        workspace,
        "search.modules.files",
        vec![
            ("module", "files".into()),
            ("query", query),
            ("page", page.to_string()),
            ("extracts", "1".into()),
            ("highlight", "1".into()),
            ("max_extract_len", "200".into()),
            ("extra_message_data", "1".into()),
            ("no_user_profile", "1".into()),
            ("count", "50".into()),
            ("file_title_only", "false".into()),
            ("query_rewrite_disabled", "false".into()),
            ("include_files_shares", "1".into()),
            ("search_context", "desktop_files_browser".into()),
            ("max_filter_suggestions", "10".into()),
            (
                "sort",
                if text.trim().is_empty() {
                    "last_engaged"
                } else {
                    "score"
                }
                .into(),
            ),
            ("sort_dir", "desc".into()),
            ("search_exclude_bots", "false".into()),
            ("search_only_my_channels", "false".into()),
            ("shared_with_you", "0".into()),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    #[test]
    fn picker_reads_use_the_source_backed_file_browser_form() {
        let session = WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.slack.com".into(),
            token: "synthetic".into(),
        };
        let request = hosted_picker_search(
            &SlackClient::default(),
            &session,
            "budget",
            HostedFileKind::Documents,
            2,
        )
        .unwrap();
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with("/api/search.modules.files")
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(
            fields,
            vec![
                ("token", "synthetic"),
                ("module", "files"),
                ("query", "budget type:documents"),
                ("page", "2"),
                ("extracts", "1"),
                ("highlight", "1"),
                ("max_extract_len", "200"),
                ("extra_message_data", "1"),
                ("no_user_profile", "1"),
                ("count", "50"),
                ("file_title_only", "false"),
                ("query_rewrite_disabled", "false"),
                ("include_files_shares", "1"),
                ("search_context", "desktop_files_browser"),
                ("max_filter_suggestions", "10"),
                ("sort", "score"),
                ("sort_dir", "desc"),
                ("search_exclude_bots", "false"),
                ("search_only_my_channels", "false"),
                ("shared_with_you", "0")
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<Vec<_>>()
        );
        assert!(
            hosted_picker_search(
                &SlackClient::default(),
                &session,
                "x",
                HostedFileKind::All,
                0
            )
            .is_err()
        );
    }
}
