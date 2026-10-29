//! Documented `search.files` read contract. Search results are not file-info proofs.
use super::*;
use crate::slack::models::{File, SearchPagination};
use std::future::Future;

#[derive(Debug, Clone)]
pub struct SearchFilesPage {
    pub items: Vec<File>,
    pub pagination: Option<SearchPagination>,
}

#[derive(serde::Deserialize)]
struct Response {
    files: Matches,
}
#[derive(serde::Deserialize)]
struct Matches {
    matches: Vec<File>,
    pagination: Option<SearchPagination>,
    paging: Option<Paging>,
}
#[derive(serde::Deserialize)]
struct Paging {
    page: Option<u32>,
    pages: Option<u32>,
    count: Option<u32>,
    total: Option<u64>,
}

pub fn search_files(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchArgs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.files",
        vec![
            ("query", args.query),
            ("count", args.count.clamp(1, 100).to_string()),
            ("page", args.page.max(1).to_string()),
            ("sort", args.sort.api().0.into()),
            ("sort_dir", args.sort.api().1.into()),
            ("highlight", "false".into()),
        ],
    )
}

pub async fn fetch_search_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchArgs,
) -> Result<SearchFilesPage, Error> {
    fetch_search_files_with(client, workspace, args, |request| {
        transport.execute(request)
    })
    .await
}

pub async fn fetch_search_files_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchArgs,
    execute: F,
) -> Result<SearchFilesPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let value = execute(search_files(client, workspace, args)).await?;
    if value["ok"] == false {
        return Err(Error::Api(
            value["error"]
                .as_str()
                .unwrap_or("file_search_failed")
                .into(),
        ));
    }
    let page: Response = decode(value, "search.files")?;
    let pagination = page.files.pagination.or_else(|| {
        page.files.paging.map(|p| SearchPagination {
            page: p.page,
            page_count: p.pages,
            per_page: p.count,
            total_count: p.total,
            ..Default::default()
        })
    });
    Ok(SearchFilesPage {
        items: page.files.matches,
        pagination,
    })
}

#[cfg(test)]
mod tests;
