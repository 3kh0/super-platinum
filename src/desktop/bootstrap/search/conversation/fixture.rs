use super::*;
use crate::state::{Overlay, SearchResultVm, ShellState};
use serde_json::json;
use super_platinum_core::slack::models::SearchPagination;

pub(in crate::bootstrap::search) fn install(shell: &mut ShellState) {
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("release".into());
    let channels: Vec<Channel> = serde_json::from_value(json!([
        {"id":"C2","name":"ship","is_channel":true},
        {"id":"D_SEARCH_PEER","is_im":true,"user":"U1"},
        {"id":"G_SEARCH_GROUP","is_mpim":true,"name":"mpdm-Maya.Chen--Jules-1"}
    ]))
    .expect("search conversation fixture");
    let workspace = &shell.core.workspaces["T1"];
    shell.search_results = channels
        .iter()
        .enumerate()
        .map(|(index, channel)| {
            let (channel_name, direct_conversation) = label(workspace, channel);
            SearchResultVm {
                direct_conversation,
                channel_id: channel.id.clone(),
                channel_name,
                ts: format!("171980040{index}.000200"),
                thread_ts: None,
                author: workspace.display_name("U2"),
                text: "The release review is ready.".into(),
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
