//! File browser request shape observed in Slack's `search.modules.files`.
use super::*;
use crate::files::{FileOwner, FilesPage, FilesQuery};

pub fn files_search(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: &FilesQuery,
    page: u32,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.modules.files",
        vec![
            ("module", "files".into()),
            ("query", query.search_text(&workspace.user_id)),
            ("page", page.max(1).to_string()),
            ("count", "50".into()),
            ("sort", query.effective_sort().api().into()),
            ("sort_dir", "desc".into()),
            (
                "shared_with_you",
                if query.owner == FileOwner::Shared {
                    "1"
                } else {
                    "0"
                }
                .into(),
            ),
            ("file_title_only", "false".into()),
            ("include_files_shares", "1".into()),
            (
                "search_context",
                if query.channel.is_some() {
                    "desktop_files_channel_tab"
                } else {
                    "desktop_files_browser"
                }
                .into(),
            ),
            ("highlight", "0".into()),
            ("extracts", "0".into()),
        ],
    )
}
pub async fn fetch_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: &FilesQuery,
    page: u32,
) -> Result<FilesPage, Error> {
    decode(
        transport
            .execute(files_search(client, workspace, query, page))
            .await?,
        "search files",
    )
}

pub fn channel_links_search(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    query: &str,
    page: u32,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "conversations.searchLinks",
        vec![
            ("channel_id", channel.into()),
            ("query", query.trim().into()),
            ("page", page.max(1).to_string()),
            ("sort", "timestamp".into()),
            ("sort_dir", "desc".into()),
        ],
    )
}

pub async fn fetch_channel_links(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    query: &str,
    page: u32,
) -> Result<crate::links::LinksPage, Error> {
    decode(
        transport
            .execute(channel_links_search(
                client, workspace, channel, query, page,
            ))
            .await?,
        "channel links",
    )
}
