//! Native loading for the workspace Threads feed; no read marks are sent here.
use crate::state::ShellState;
use super_platinum_core::domain::ThreadsState;

pub fn visible_state(shell: &ShellState) -> Option<&ThreadsState> {
    shell
        .core
        .threads_view
        .matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        .then_some(&shell.core.threads_view)
}

mod read;
mod unread;
pub(crate) use read::ProfileMember;
pub(crate) use read::ProfileOpen;
pub(crate) use read::accept_read_barrier;
pub(crate) use read::copy_link;
pub(crate) use read::publish_profile_refresh;
pub(crate) use read::reaction_toggle;
pub(crate) use read::reply_visible_limit;
pub(crate) use read::timestamp;
pub(crate) use read::{ContextPrepared, finish_context, prepare_context};
pub(crate) use read::{document_author_id, schedule_document_names};
pub(crate) fn earlier_layout(shell: &ShellState) -> bool {
    shell.thread_feed_reads.earlier_layout
}
pub use read::load;
pub(crate) use read::{MarkAll, has_unread_threads, mark_all_status};
pub(crate) use read::{Mode, Reads, Source, load_reconnect, unread_marker};

/// Demotion only, after the authenticated socket's guarded Core mutation.
/// No event/public vector may mint a native page or re-add a retired row key.
pub(crate) fn after_native_mutation(shell: &ShellState, team: &str, generation: u64) {
    if shell.core.active_team.as_deref() == Some(team)
        && shell
            .core
            .workspaces
            .get(team)
            .is_some_and(|w| w.rt_generation == generation)
    {
        read::retire_after_native_mutation(shell);
    }
}

// Use the cursor retained by the revision-aware merge, never the stale page's
// last_read when a socket mark arrived while it was in flight.
fn reconcile_cursors(
    core: &mut super_platinum_core::CoreAppState,
    keys: &[super_platinum_core::domain::ThreadKey],
) {
    for key in keys {
        if let Some(cursor) = core.thread_cursor(key).map(str::to_owned) {
            crate::realtime::activity_counts::mark_thread_read(
                core, &key.0, &key.1, &key.2, &cursor,
            );
        }
    }
}

pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    // Presentation replacement cannot reuse a native accepted-row publication.
    shell.thread_feed_reads = Default::default();
    crate::thread_unread::list_fixture(shell);
    if name.starts_with("thread-feed-copy-link") {
        copy_link::fixture(shell);
    }
    if name.starts_with("thread-feed-timestamp") {
        timestamp::fixture(shell);
    }
    if name.starts_with("thread-feed-root-first-add-custom") {
        reaction_toggle::first_add::custom_fixture(shell);
    } else if name.starts_with("thread-feed-root-first-add") {
        reaction_toggle::first_add::fixture(shell);
    }
    let epoch = shell.core.account_epoch;
    let team = shell.core.active_team.clone().unwrap();
    let items = std::mem::take(&mut shell.core.threads_view.items);
    let feed = &mut shell.core.threads_view;
    feed.ensure_scope(epoch, &team);
    feed.items = items;
    if let Some(item) = feed.items.first_mut() {
        item.root_msg.reply_count = Some(147);
    }
    feed.loaded = true;
    feed.has_more = true;
    feed.next_cursor = Some("1719800403.000000".into());
    if name.starts_with("thread-feed-earlier-local") {
        // Inert layout only. This flag cannot construct a native window/action.
        shell.thread_feed_reads.earlier_layout = true;
    }
    if name.starts_with("thread-feed-rich-root") {
        let root = &mut feed.items[0].root_msg;
        root.text = Some("Fallback must not duplicate the rich body".into());
        root.blocks = vec![
            serde_json::json!({"type":"rich_text","elements":[
                {"type":"rich_text_section","elements":[
                    {"type":"text","text":"Release notes\n","style":{"bold":true}},
                    {"type":"text","text":"The accepted root stays readable across multiple paragraphs."}]},
                {"type":"rich_text_list","style":"ordered","elements":[
                    {"type":"rich_text_section","elements":[{"type":"text","text":"Review the changes"}]},
                    {"type":"rich_text_section","elements":[{"type":"link","url":"https://fixture.invalid/notes","text":"Read the notes"}]}]},
                {"type":"rich_text_preformatted","elements":[{"type":"text","text":"cargo test --locked"}]},
                {"type":"rich_text_quote","elements":[{"type":"text","text":"Keep the release discussion in context."}]}
            ]}),
            serde_json::json!({"type":"image","image_url":"https://fixture.invalid/root.png","alt_text":"Release diagram"}),
            serde_json::json!({"type":"actions","elements":[{"type":"button","action_id":"must-stay-inert","text":{"type":"plain_text","text":"Review action"},"url":"https://fixture.invalid/action"}]}),
        ];
    }
    if name.starts_with("thread-feed-replies")
        || name.starts_with("thread-feed-unread")
        || name.starts_with("thread-feed-reaction-groups")
    {
        let item = &mut feed.items[0];
        let root = item.root_msg.ts.clone().unwrap();
        let channel = item.root_msg.channel.clone().unwrap();
        item.latest_replies = vec![
            serde_json::from_value(serde_json::json!({"channel":channel,"thread_ts":root,"ts":"1719800401.000000","user":"U1","text":"Fallback must not duplicate reply blocks","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Reply context\nA second readable paragraph.","style":{"bold":true}},{"type":"link","url":"https://fixture.invalid/reply","text":"Read the reply notes"}]}]},{"type":"image","image_url":"https://fixture.invalid/reply.png","alt_text":"Reply diagram"},{"type":"actions","elements":[{"type":"button","action_id":"inert-reply-action","text":{"type":"plain_text","text":"Reply action"},"url":"https://fixture.invalid/reply-action"}]}]})).unwrap(),
            serde_json::from_value(serde_json::json!({"channel":channel,"thread_ts":root,"ts":"1719800403.000000","user":"U1","text":"Latest supplied reply"})).unwrap(),
        ];
        item.unread_replies = item.latest_replies[1..].to_vec();
        if name.starts_with("thread-feed-replies-reactions") {
            item.latest_replies[0].reactions = serde_json::from_value(serde_json::json!([
                {"name":"thumbsup","count":2,"users":["U1","U2"]},
                {"name":"shipit","count":1,"users":["U1"]}
            ]))
            .unwrap();
            item.unread_replies = item.latest_replies.clone();
        }
    }
    if name.starts_with("thread-feed-root-reactions")
        || name.starts_with("thread-feed-reaction-groups")
    {
        let root = &mut feed.items[0].root_msg;
        root.kind = Some("message".into());
        root.team = Some(team.clone());
        root.reactions = serde_json::from_value(serde_json::json!([
            {"name":"thumbsup","count":2,"users":["U1","U2"]},
            {"name":"shipit","count":1,"users":["U1"]}
        ]))
        .unwrap();
    }
    if name.starts_with("thread-feed-reaction-groups") {
        let item = &mut feed.items[0];
        item.root_msg.reactions=serde_json::from_value(serde_json::json!([
            {"name":"thumbsup::skin-tone-2","count":2,"users":["U1","U2"]},
            {"name":"shipit","count":1,"users":["U1"],"url":"https://fixture.invalid/group-reaction.png"},
            {"name":"thumbsup::skin-tone-5","count":3,"users":["U3","U4","U5"]}
        ])).unwrap();
        item.latest_replies[0].reactions = serde_json::from_value(serde_json::json!([
            {"name":"wave::skin-tone-2","count":1,"users":["U1"]},
            {"name":"wave::skin-tone-5","count":2,"users":["U2","U3"]}
        ]))
        .unwrap();
        item.unread_replies = item.latest_replies.clone();
    }
    if name.starts_with("thread-feed-reaction-groups-people") {
        let item = &mut feed.items[0];
        item.root_msg.reactions = serde_json::from_value(serde_json::json!([
            {"name":"thumbsup::skin-tone-2","count":9,"users":["U1","U2","U1","","U6","U7","U8","U9","U10","U0"]},
            {"name":"thumbsup::skin-tone-5","count":2,"users":["U0","U2"],"url":"https://fixture.invalid/group-reaction.png"}
        ])).unwrap();
        item.latest_replies[0].reactions = serde_json::from_value(serde_json::json!([
            {"name":"wave","count":2,"users":["U0","U1"]}
        ]))
        .unwrap();
        item.unread_replies = item.latest_replies.clone();
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        for n in 6..=10 {
            let user = serde_json::from_value(
                serde_json::json!({"id":format!("U{n}"),"name":format!("Reactor {n}"),"profile":{"image_72":format!("https://fixture.invalid/reactor-{n}.png")}}),
            )
            .unwrap();
            workspace.users.insert(format!("U{n}"), user);
        }
        // Synthetic identity only for this inert presentation fixture, with no
        // transport, token, accepted row or action receipt installed here.
        shell.core.session = Some(super_platinum_core::config::Session {
            d_cookie: String::new(),
            unified_user_client_enabled: Default::default(),
            workspaces: [(
                team.clone(),
                super_platinum_core::config::WorkspaceSession {
                    team_id: team.clone(),
                    enterprise_id: None,
                    user_id: workspace.self_user_id.clone(),
                    name: "Fixture".into(),
                    url: "https://fixture.invalid".into(),
                    token: String::new(),
                },
            )]
            .into(),
        });
    }
    match name {
        "thread-feed-refresh-retry"
        | "thread-feed-refresh-retry-narrow"
        | "thread-feed-older-retry"
        | "thread-feed-older-retry-narrow"
        | "thread-feed-initial-retry"
        | "thread-feed-initial-retry-narrow" => {
            if name.contains("initial") {
                feed.items.clear();
                feed.loaded = false;
                feed.has_more = false;
                feed.next_cursor = None;
            }
            let request = feed.begin(name.contains("older")).unwrap();
            feed.fail(&request);
        }
        "thread-feed-empty" => {
            feed.items.clear();
            feed.has_more = false;
        }
        "thread-feed-error" => feed.failed = true,
        "thread-feed-loading" => feed.loading = true,
        "thread-feed-updated" => feed.dirty = true,
        _ => {}
    }
    if name.starts_with("thread-feed-unread") {
        unread::fixture(shell, name.starts_with("thread-feed-unread-read"));
    }
    if name.starts_with("thread-feed-mark-all") {
        // Presentation only: counts do not mint the native Action/Accepted.
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .thread_counts
            .apply_live(super_platinum_core::slack::models::ThreadCounts {
                has_unreads: true,
                ..Default::default()
            });
    }
    if name.starts_with("thread-feed-profile-reading") {
        // Inert pane layout only. A synthetic hint never mints the original
        // actor/member capability needed by live author buttons.
        let root = &mut shell.core.threads_view.items[0].root_msg;
        root.user = Some("UREADER".into());
        root.kind = Some("message".into());
        root.text = Some("Read the author's details alongside this thread.".into());
        let hint=serde_json::from_value(serde_json::json!({"id":"UREADER","name":"reader","profile":{"display_name":"Jordan Reader","title":"Product designer","pronouns":"they/them","email":"reader@fixture.invalid","phone":"555 0100"}})).unwrap();
        let _ = crate::bootstrap::profile_search::show(shell, hint);
        if let Some(pane) = shell.core.profile_pane.as_mut() {
            pane.loading = false;
        }
    }
    if name.starts_with("thread-feed-document-authors") {
        // Passive layout only; fixture users cannot mint member knownness or
        // document-profile/IO authority. Real names need the original reader.
        let feed = &mut shell.core.threads_view;
        feed.items.truncate(1);
        let item = &mut feed.items[0];
        let root = &mut item.root_msg;
        root.kind = Some("message".into());
        root.subtype = None;
        root.user = Some("UPOSTER".into());
        root.text = Some("A document comment attributed to its editor.".into());
        root.blocks.clear();
        root.attachments.clear();
        root.files.clear();
        root.bot_id = None;
        root.bot_profile = None;
        root.message = None;
        root.extra.insert(
            "document_comment".into(),
            serde_json::json!({"most_recent_editor":"UEDITOR","authors":["UAUTHOR"]}),
        );
        let ts = root.ts.clone().unwrap();
        let seconds = ts.split('.').next().unwrap().parse::<u64>().unwrap() + 1;
        item.latest_replies=(1..=2).map(|n|serde_json::from_value(serde_json::json!({
            "type":"message","channel":root.channel.clone(),"team":team.clone(),"thread_ts":ts.clone(),
            "ts":format!("{seconds}.{n:06}"),"user":"UPOSTER",
            "text":if n==1{"First-author fallback stays separate from the posting user."}else{"An unresolved editor stays an explicit ID."},
            "document_comment":if n==1{serde_json::json!({"authors":["UAUTHOR","UEDITOR"]})}else{serde_json::json!({"most_recent_editor":"UUNRESOLVED"})}
        })).unwrap()).collect();
        item.unread_replies.clear();
        root.reply_count = Some(2);
        root.latest_reply = item.latest_replies.last().and_then(|m| m.ts.clone());
        feed.has_more = false;
        feed.next_cursor = None;
    }
    if matches!(name, "thread-feed-edited" | "thread-feed-edited-narrow") {
        // Disconnected presentation data only, never an accepted page/action.
        shell.core.transport = None;
        shell.core.session = None;
        let feed = &mut shell.core.threads_view;
        let channel = feed.items[0].root_msg.channel.clone().unwrap();
        let root = feed.items[0].root_msg.ts.clone().unwrap();
        let seconds = root.split('.').next().unwrap().parse::<u64>().unwrap();
        let edited = format!("{}.000001", seconds + 10);
        let first = format!("{}.000001", seconds + 1);
        let second = format!("{}.000001", seconds + 2);
        let third = format!("{}.000001", seconds + 3);
        feed.items = vec![serde_json::from_value(serde_json::json!({
            "root_msg":{
                "type":"message","channel":channel,"team":team,"ts":root,
                "user":"U1","reply_count":3,"latest_reply":third,
                "text":"Fallback must not duplicate the edited rich body",
                "edited":{"ts":edited,"user":"U1"},
                "blocks":[{"type":"rich_text","elements":[
                    {"type":"rich_text_section","elements":[
                        {"type":"text","text":"Edited release plan\n","style":{"bold":true}},
                        {"type":"text","text":"The final paragraph stays readable."}
                    ]}
                ]}]
            },
            "latest_replies":[
                {"type":"message","channel":channel,"team":team,"thread_ts":root,
                 "ts":first,"user":"U1","text":"Edited reply fallback",
                 "edited":{"ts":edited},
                 "blocks":[{"type":"rich_text","elements":[
                    {"type":"rich_text_section","elements":[
                        {"type":"text","text":"The reply was clarified.","style":{"italic":true}}
                    ]}
                 ]}]},
                {"type":"message","channel":channel,"team":team,"thread_ts":root,
                 "ts":second,"user":"U1","text":"Unedited reply control",
                 "blocks":[{"type":"rich_text","elements":[
                    {"type":"rich_text_section","elements":[
                        {"type":"text","text":"This reply has no edit metadata."}
                    ]}
                 ]}]},
                {"type":"message","channel":channel,"team":team,"thread_ts":root,
                 "ts":third,"user":"U1","text":"Unsupported placement control",
                 "edited":{"ts":edited},
                 "blocks":[{"type":"rich_text","elements":[
                    {"type":"rich_text_quote","elements":[
                        {"type":"text","text":"Quoted content keeps its original body."}
                    ]}
                 ]}]}
            ],
            "unread_replies":[]
        }))
        .expect("static passive edited fixture")];
        feed.has_more = false;
        feed.next_cursor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fetched_thread_cursor_cannot_override_an_intervening_mark_in_activity() {
        let mut core = super_platinum_core::CoreAppState::new(Default::default());
        core.active_team = Some("T".into());
        core.activity.ensure_scope(core.account_epoch, "T");
        core.threads_view.ensure_scope(core.account_epoch, "T");
        let key = ("T".into(), "C".into(), "1.0".into());
        for ts in ["2.0", "4.0"] {
            core.activity.upsert(
                serde_json::from_value(serde_json::json!({
                    "key":ts,"feed_ts":ts,"is_unread":true,
                    "item":{"type":"at_user","message":{"channel":"C","thread_ts":"1.0","ts":ts}}
                }))
                .unwrap(),
            );
        }
        let request = core.threads_view.begin(false).unwrap();
        core.apply_thread_mark(&key, "2.0");
        let page = serde_json::from_value(serde_json::json!({"threads":[{
            "root_msg":{"channel":"C","ts":"1.0","last_read":"4.0"},
            "latest_replies":[{"ts":"2.0"},{"ts":"4.0"}]
        }]}))
        .unwrap();
        assert!(core.apply_thread_feed_page("T", &request, page, &Default::default()));
        reconcile_cursors(&mut core, &[key]);
        assert_eq!(core.activity.unread_count(), 1);
        assert!(
            core.activity
                .items
                .iter()
                .find(|item| item.key == "4.0")
                .unwrap()
                .is_unread
        );
    }

    #[test]
    fn switching_workspace_releases_a_pending_feed_request() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture(&mut shell, "thread-feed");
        let old_team = shell.core.active_team.clone().unwrap();
        let request = shell.core.threads_view.begin(false).unwrap();
        let other = shell
            .workspaces
            .iter()
            .position(|w| w.id != old_team)
            .unwrap();
        let other_id = shell.workspaces[other].id.clone();
        let mut workspace = shell.core.workspaces[&old_team].clone();
        workspace.team_id = other_id.clone();
        shell.core.workspaces.insert(other_id, workspace);
        assert!(shell.select_workspace(other));
        assert!(!shell.core.threads_view.loading);
        assert!(shell.core.threads_view.items.is_empty());
        assert_ne!(shell.core.threads_view.load_seq, request.sequence);
    }
}
