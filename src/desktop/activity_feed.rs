//! Native Activity loading. Fetching or paging never marks notifications read.
mod admission;
pub(crate) mod check;
pub(crate) use admission::Navigation;
mod navigation;
pub(crate) use navigation::thread_destination;

use crate::state::{MainView, ShellState};
mod read;
pub use read::load;
pub(crate) use read::{Mode, Source, load_reconnect, schedule_failed_after_outage};

pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    shell.main_view = MainView::Activity;
    let feed = &mut shell.core.activity;
    feed.loaded = true;
    feed.next_cursor = Some("older-notifications".into());
    match name {
        "activity-refresh-retry"
        | "activity-refresh-retry-narrow"
        | "activity-older-retry"
        | "activity-older-retry-narrow" => recovery_fixture(shell, name.contains("older")),
        "activity-navigation" | "activity-navigation-narrow" => check::install(shell),
        "activity-loading" => {
            feed.items.clear();
            feed.loaded = false;
            feed.loading = true;
            feed.next_cursor = None;
        }
        "activity-error" => {
            feed.items.clear();
            feed.loaded = false;
            feed.failed = true;
            feed.next_cursor = None;
        }
        "activity-more-error" => feed.failed = true,
        "activity-empty" => {
            feed.items.clear();
            feed.next_cursor = None;
        }
        _ => {}
    }
}

fn recovery_fixture(shell: &mut ShellState, older: bool) {
    let team = shell.core.active_team.clone().unwrap();
    shell.overlay = None;
    let epoch = shell.core.account_epoch;
    let feed = &mut shell.core.activity;
    *feed = Default::default();
    feed.ensure_scope(epoch, &team);
    let first = feed.begin(false).unwrap();
    let page = serde_json::from_value(serde_json::json!({"items":[{
        "key":"retained-activity-read","feed_ts":"1719801500.000800","is_unread":false,
        "item":{"type":"at_user","message":{"channel":"C1","ts":"1719801500.000800","text":"Retained notification"}}
    }],"response_metadata":{"next_cursor":"opaque/+?=&older"}})).unwrap();
    feed.apply_page(&first, page);
    feed.selected = Some("retained-activity-read".into());
    feed.unread_only = true;
    let failed = feed.begin(older).unwrap();
    feed.fail(&failed);
}
