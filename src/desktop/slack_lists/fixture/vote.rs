//! Named disconnected Vote surfaces. The existing backend owns all updates.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-vote-cells"
            | "slack-list-vote-cells-narrow"
            | "slack-list-vote-board"
            | "slack-list-vote-board-narrow"
            | "slack-list-vote-detail"
            | "slack-list-vote-detail-narrow"
    ));
    view_filter::seed(shell, name);
    let user = shell.self_account.user_id.clone();
    let team = shell.core.active_team.clone().unwrap();
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Vote workflow");
    for (id, key, label, options) in [
        (
            "C_VOTE",
            "support",
            "Support",
            json!({"emoji":":sparkling_heart:"}),
        ),
        (
            "C_VOTE_STANDARD",
            "approval",
            "Approval",
            json!({"emoji":":heart:","emoji_team_id":team}),
        ),
        (
            "C_VOTE_CUSTOM",
            "custom_vote",
            "Custom vote",
            json!({"emoji":":fixture_vote:","emoji_team_id":team}),
        ),
    ] {
        f.document["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"key":key,"name":label,"type":"vote","options":options}));
    }
    // Keep the relevant controls within the first viewport without dropping raw fields.
    f.document["list_metadata"]["views"][0]["columns"] = json!([
        {"id":"C_NAME","key":"NAME","visible":true},
        {"id":"C_VOTE","key":"support","visible":true},
        {"id":"C_VOTE_STANDARD","key":"approval","visible":true},
        {"id":"C_VOTE_CUSTOM","key":"custom_vote","visible":true}
    ]);
    f.document["list_metadata"]["views"][0]["sorts"] = json!([]);
    if name.contains("board") {
        f.document["list_metadata"]["views"][0]["type"] = json!("board");
    }
    for (i, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        for key in ["support", "approval", "custom_vote"] {
            if i % 4 != 0 {
                row["fields"].as_array_mut().unwrap().push(json!({"key":key,"user":match i%4 {1=>json!([user,"U_OTHER"]),2=>json!(["U_OTHER"]),_=>json!([])}}));
            }
        }
    }
    refresh(shell);
    if name.contains("detail") {
        let v = shell.slack_lists.view.as_ref().unwrap();
        let (instance, visit, layout) = (
            v.rows[0].instance_id.clone(),
            v.generation,
            v.layout.generation,
        );
        assert!(super::super::record_detail::open_shell(
            shell, &instance, visit, layout
        ));
    }
}
