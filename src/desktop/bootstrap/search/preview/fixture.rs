use crate::state::{Overlay, SearchResultVm, ShellState};
use serde_json::json;
use super_platinum_core::slack::models::{Message, SearchPagination};

pub(in crate::bootstrap::search) fn install(shell: &mut ShellState) {
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("release".into());
    let messages: Vec<Message> = serde_json::from_value(json!([
        {"user":"U1","ts":"1719800400.000200","attachments":[{
            "pretext":"Release notice","title":"Desktop release report",
            "text":"The release is ready for review.",
            "fields":[{"title":"Checks","value":"All required checks passed"}]
        }]},
        {"user":"U2","ts":"1719800401.000200","text":"Review the release notes before publishing.",
            "files":[{"id":"F_SEARCH_NOTES","name":"release-notes.txt","title":"Release notes"}]},
        {"user":"U1","ts":"1719800402.000200","attachments":[{
            "image_url":"https://example.test/search-release-preview.png"
        }]}
    ]))
    .expect("attachment search fixture");
    let workspace = &shell.core.workspaces["T1"];
    shell.search_results = messages
        .iter()
        .map(|message| {
            let vm = crate::message_vm::message_vm(workspace, message, &shell.media);
            SearchResultVm {
                direct_conversation: false,
                channel_id: "C2".into(),
                channel_name: "ship".into(),
                ts: vm.ts.clone(),
                thread_ts: None,
                author: vm.author.clone(),
                text: super::text(&vm),
            }
        })
        .collect();
    let request = shell
        .search_pager
        .begin(shell.core.account_epoch, "T1", "release", false)
        .unwrap();
    shell.search_pager.complete(
        &request,
        Some(&SearchPagination {
            page: Some(1),
            page_count: Some(1),
            total_count: Some(3),
            ..Default::default()
        }),
    );
}
