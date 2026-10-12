use super::{Context, Details, ShellState, api, failure};
use dioxus::prelude::*;
use super_platinum_core::slack::Error;

pub(super) fn revision(shell: &ShellState, channel: &str) -> Option<(u64, u64)> {
    let workspace = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))?;
    Some((
        workspace.rt_generation,
        workspace.channel_lifecycle.members_revision(channel),
    ))
}

pub(super) async fn load_members(
    mut state: Signal<ShellState>,
    mut details: Signal<Details>,
    channel: String,
    replace: bool,
) {
    if details.peek().members_loading {
        return;
    }
    if crate::fixture::is_fixture() {
        load_fixture(state, details, &channel, replace);
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    details.write().members_loading = true;
    details.write().error.clear();
    if replace {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    if !ctx.current(&state.peek()) {
        return;
    }
    let Some(started) = revision(&state.peek(), &channel) else {
        return;
    };
    let replace = replace || needs_replacement(&details.peek(), started);
    details.write().members_revision = Some(started);
    let cursor = if replace {
        None
    } else {
        details.peek().members.cursor.clone()
    };
    let result = api::fetch_conversation_members(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        &channel,
        cursor.as_deref(),
    )
    .await;
    if !ctx.current(&state.peek()) {
        return;
    }
    if let Ok(page) = &result {
        if let Some(workspace) = state.write().core.workspaces.get_mut(&ctx.session.team_id) {
            for user in &page.users {
                workspace.users.insert(user.id.clone(), user.clone());
            }
        }
        let missing = page
            .members
            .iter()
            .filter(|id| {
                state
                    .peek()
                    .core
                    .workspaces
                    .get(&ctx.session.team_id)
                    .is_none_or(|w| !w.users.contains_key(*id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            if let Ok(users) =
                api::fetch_users_info(&ctx.transport, &ctx.client, &ctx.session, missing).await
            {
                if !ctx.current(&state.peek()) {
                    return;
                }
                if let Some(workspace) = state.write().core.workspaces.get_mut(&ctx.session.team_id)
                {
                    for user in users {
                        workspace.users.insert(user.id.clone(), user);
                    }
                }
            }
        }
    }
    let current = revision(&state.peek(), &channel);
    finish_page(&mut details.write(), result, replace, started, current);
}

// A failed refresh leaves the old rows readable, but its cursor must never be
// used to append pages from the new roster to the old snapshot.
fn needs_replacement(data: &Details, revision: (u64, u64)) -> bool {
    data.members_snapshot_revision
        .is_some_and(|previous| previous != revision)
}

fn finish_page(
    data: &mut Details,
    result: Result<api::ConversationMembersPage, Error>,
    replace: bool,
    started: (u64, u64),
    current: Option<(u64, u64)>,
) {
    data.members_loading = false;
    // A join/leave or reconnect after this request began invalidates the page.
    // The reactive observer will request the latest version after this returns.
    if current != Some(started) {
        return;
    }
    match result {
        Ok(page) => {
            if replace {
                data.members = Default::default();
            }
            data.members_loaded = true;
            data.members_snapshot_revision = Some(started);
            if !data.members.apply(page) {
                data.error = "Slack repeated a member page. Reload members to try again.".into();
            }
        }
        Err(error) => data.error = failure(&error),
    }
}

fn load_fixture(
    mut state: Signal<ShellState>,
    mut details: Signal<Details>,
    channel: &str,
    replace: bool,
) {
    let started = revision(&state.peek(), channel).unwrap();
    let remote = std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("member-live-refresh");
    let other = if remote && started.1 > 0 { "U2" } else { "U1" };
    details.write().members_revision = Some(started);
    finish_page(
        &mut details.write(),
        Ok(api::ConversationMembersPage {
            members: vec!["U0".into(), other.into()],
            ..Default::default()
        }),
        replace,
        started,
        Some(started),
    );
    if remote && started.1 == 0 {
        let channel = channel.to_owned();
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let event = super_platinum_core::slack::realtime::parse_event(
                &serde_json::json!({
                    "type":"member_left_channel", "channel":channel, "user":"U1", "event_ts":"100.0"
                })
                .to_string(),
            )
            .unwrap();
            state
                .write()
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .apply_channel_event(&event);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(ids: &[&str], cursor: Option<&str>) -> api::ConversationMembersPage {
        serde_json::from_value(
            serde_json::json!({"members":ids,"response_metadata":{"next_cursor":cursor}}),
        )
        .unwrap()
    }
    #[test]
    fn stale_member_pages_and_errors_cannot_restore_a_departed_person() {
        let mut data = Details::default();
        data.members.confirm_member("CURRENT", true);
        for current in [Some((1, 2)), Some((2, 1)), None] {
            finish_page(
                &mut data,
                Ok(page(&["DEPARTED"], Some("stale"))),
                true,
                (1, 1),
                current,
            );
            assert_eq!(data.members.ids, ["CURRENT"]);
            assert_eq!(data.members.cursor, None);
            finish_page(
                &mut data,
                Err(Error::Offline("fixture".into())),
                true,
                (1, 1),
                current,
            );
            assert!(data.error.is_empty());
        }
        finish_page(
            &mut data,
            Ok(page(&["NEW"], Some("next"))),
            true,
            (2, 2),
            Some((2, 2)),
        );
        assert_eq!(data.members.ids, ["NEW"]);
        assert_eq!(data.members.cursor.as_deref(), Some("next"));
        finish_page(
            &mut data,
            Ok(page(&["NEW", "NEXT"], None)),
            false,
            (2, 2),
            Some((2, 2)),
        );
        assert_eq!(data.members.ids, ["NEW", "NEXT"]);
    }
    #[test]
    fn failed_refresh_keeps_existing_members_and_cursor_for_retry() {
        let mut data = Details::default();
        data.members.apply(page(&["KEEP"], Some("next")));
        data.members_snapshot_revision = Some((1, 0));
        finish_page(
            &mut data,
            Err(Error::Offline("fixture".into())),
            true,
            (1, 1),
            Some((1, 1)),
        );
        assert_eq!(data.members.ids, ["KEEP"]);
        assert_eq!(data.members.cursor.as_deref(), Some("next"));
        assert!(!data.error.is_empty());
        assert!(!data.members_loading);
        assert!(needs_replacement(&data, (1, 1)));
        assert!(!needs_replacement(&data, (1, 0)));
    }
}
