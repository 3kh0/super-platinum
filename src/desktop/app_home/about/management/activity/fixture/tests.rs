use super::*;
use super_platinum_core::{config::WorkspaceSession, slack::SlackClient};

#[test]
fn channel_fixture_binds_account_before_about_owner_and_real_tab_admission() {
    use crate::{
        channel_tabs::selection::{self, Route},
        state::ChannelTab,
    };
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    install_case(&mut shell, "channel");
    assert_eq!(
        shell.core.active_account.as_deref(),
        Some("synthetic-workflow-activity-channel")
    );
    assert!(
        shell
            .app_home
            .about
            .surface
            .as_ref()
            .unwrap()
            .owner
            .current(&shell)
    );
    assert!(crate::app_home::eligible(&shell));
    shell.channel_tab = ChannelTab::Messages;
    assert!(selection::record(
        &mut shell,
        Route::Resource(ChannelTab::AppAbout),
        None
    ));
    shell.core.active_account = None;
    assert!(!selection::record(
        &mut shell,
        Route::Resource(ChannelTab::AppAbout),
        None
    ));
}

fn requests() -> Vec<PreparedRequest> {
    let owner = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "UWORKFLOW".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let client = SlackClient::default();
    vec![
        ActivityQuery::new("WF_REVIEW", ActivityFilters::default())
            .unwrap()
            .request()
            .request(&client, &owner),
        ActivityDetailRequest::new("WF_REVIEW", FIRST)
            .unwrap()
            .request()
            .request(&client, &owner),
    ]
}

#[test]
fn actual_activity_plans_are_admitted_without_generic_retry_permission() {
    for request in requests() {
        assert!(
            !request.retry_safe(),
            "Do not change transport policy for the fixture"
        );
        let (method, args) = fields(&request).unwrap();
        assert_eq!(args["workflow_id"], "WF_REVIEW");
        assert!(args.get("token").is_none());
        let raw = reply(method, &args, false).unwrap();
        if method == "workflows.activity.list" {
            let query = ActivityQuery::new("WF_REVIEW", ActivityFilters::default()).unwrap();
            assert_eq!(
                ActivityPage::decode(&query, raw).unwrap().rows()[0]["workflow_execution_id"],
                FIRST
            );
        } else {
            let detail = ActivityDetailRequest::new("WF_REVIEW", FIRST)
                .unwrap()
                .decode(raw)
                .unwrap();
            assert_eq!(detail.status(), Some("PENDING"));
        }
    }
}

#[test]
fn closed_activity_allowlist_rejects_foreign_host_method_and_get() {
    let original = requests().remove(0);
    for (method, url) in [
        ("GET", original.url.as_str()),
        ("POST", "https://foreign.test/api/workflows.activity.list"),
        (
            "POST",
            "https://example.test/api/functions.workflows.update",
        ),
        (
            "POST",
            "https://example.test/api/workflows.activity.list?foreign=1",
        ),
    ] {
        let mut request = original.clone();
        request.method = method;
        request.url = url.into();
        assert!(fields(&request).is_err());
    }
}

#[test]
fn closed_activity_requires_actual_form_workflow_and_source_reason() {
    for original in requests() {
        let mut nonform = original.clone();
        nonform.body = RequestBody::Json(json!({"workflow_id":"WF_REVIEW"}));
        assert!(fields(&nonform).is_err());
        for key in ["workflow_id", "_x_reason"] {
            let mut request = original.clone();
            let RequestBody::Form(form) = &mut request.body else {
                unreachable!()
            };
            form.iter_mut().find(|(name, _)| name == key).unwrap().1 = "foreign".into();
            assert!(fields(&request).is_err());
        }
    }
}

#[test]
fn special_channel_seed_preserves_ordinary_case_and_local_flags() {
    for special in [false, true] {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install_case(
            &mut shell,
            if special {
                "channel-special"
            } else {
                "channel"
            },
        );
        let workspace = &shell.core.workspaces["TWORKFLOW"];
        let known = &workspace.channels[channel::KNOWN];
        assert_eq!(known.name.as_deref(), Some("activity-known"));
        assert_eq!(known.is_private, special);
        assert!(!known.is_archived);
        assert!(!workspace.channels.contains_key(channel::MISSING));
        assert_eq!(workspace.channels.contains_key(channel::ARCHIVED), special);
        assert_eq!(
            workspace.channels.contains_key(channel::PRIVATE_ARCHIVED),
            special
        );
        if special {
            for (id, private, name) in [
                (channel::ARCHIVED, false, "activity-archived"),
                (channel::PRIVATE_ARCHIVED, true, "activity-private-archived"),
            ] {
                let row = &workspace.channels[id];
                assert_eq!(row.name.as_deref(), Some(name));
                assert!(row.is_archived);
                assert_eq!(row.is_private, private);
                assert_eq!(row.extra["is_member"], true);
            }
        }
        assert!(
            shell
                .app_home
                .about
                .surface
                .as_ref()
                .unwrap()
                .owner
                .current(&shell)
        );
    }
}

#[test]
fn special_channel_message_retains_original_missing_occurrence_and_opaque_step() {
    let original = json!({"trigger_message":{"template":"Open {known} then {known}",
        "variables":{"known":{"type":"CHANNEL","value":channel::KNOWN}}},
        "step_events":[{"step_execution_id":"STEP_REVIEW","outcome_message":{
            "template":"Check {missing}","variables":{"missing":{"type":"CHANNEL","value":channel::MISSING}}},
            "future":{"preserved":17}}],"opaque":{"keep":[false,0,null]}});
    let mut row = original.clone();
    channel::special_messages(&mut row);
    assert_eq!(row["trigger_message"], original["trigger_message"]);
    assert_eq!(row["step_events"][0], original["step_events"][0]);
    assert_eq!(row["opaque"], original["opaque"]);
    assert_eq!(row["step_events"].as_array().unwrap().len(), 2);
    let message = &row["step_events"][1]["outcome_message"];
    assert_eq!(
        message["template"],
        "Open {archived} then {privateArchived}"
    );
    assert_eq!(message["variables"]["archived"]["value"], channel::ARCHIVED);
    assert_eq!(
        message["variables"]["privateArchived"]["value"],
        channel::PRIVATE_ARCHIVED
    );
}
