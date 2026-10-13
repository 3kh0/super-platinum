//! Disconnected ordinary people fields, including uncached selected IDs.
use super::*;
use super_platinum_core::slack::models::User;

pub(crate) struct UserEditSnapshot {
    document: Value,
    records: Value,
    users: std::collections::HashMap<String, User>,
}
pub(crate) fn add_user_edits(shell: &mut ShellState) -> Option<UserEditSnapshot> {
    if !enabled(shell) {
        return None;
    }
    let team = shell.core.active_team.clone()?;
    let workspace = shell.core.workspaces.get_mut(&team)?;
    let users = workspace.users.clone();
    for (id, name, extra) in [
        ("U00000001", "Alice Example", json!({})),
        ("U00000002", "Bob Guest", json!({"is_restricted":true})),
        ("W00000003", "Whitney External", json!({"is_stranger":true})),
        ("U00000004", "Automation Bot", json!({"is_bot":true})),
        ("U00000005", "Deleted Person", json!({"deleted":true})),
        (
            "U00000006",
            "Profile Only",
            json!({"is_profile_only_user":true}),
        ),
        ("USLACKBOT", "Slackbot", json!({})),
    ] {
        let mut raw = json!({"id":id,"name":name,"real_name":name,"profile":{"display_name":name}});
        raw.as_object_mut()?.extend(extra.as_object()?.clone());
        workspace
            .users
            .insert(id.into(), serde_json::from_value(raw).ok()?);
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = UserEditSnapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
        users,
    };
    fixture.document["list_metadata"]["schema"].as_array_mut()?.extend([
        json!({"id":"C_EDIT_USER","key":"edit_user","name":"Owner","type":"user","options":{"format":"single_entity"}}),
        json!({"id":"C_EDIT_USERS","key":"edit_users","name":"Reviewers","type":"user","options":{"format":"multi_entity"}}),
        json!({"id":"C_EDIT_TASK_USER","key":"edit_task_user","name":"Task assignee","type":"todo_assignee"}),
    ]);
    fixture.document["list_metadata"]["views"].as_array_mut()?.push(json!({"id":"V_EDIT_USERS","name":"People editing","type":"table","position":"501","columns":[
        {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_EDIT_USER","key":"edit_user","visible":true},
        {"id":"C_EDIT_USERS","key":"edit_users","visible":true},{"id":"C_EDIT_TASK_USER","key":"edit_task_user","visible":true}
    ],"filters":[{"key":"estimate","column_id":"C_NUMBER","operator":"less_than_or_equal","values":[2]}],"sorts":[]}));
    for record in fixture.records["records"].as_array_mut()? {
        if record["id"] == "R_LIST_000" {
            // Earlier native checks deliberately change Estimate. Give this
            // isolated saved view its own bounded filter seed; snapshot restore
            // below puts the predecessor's exact record back afterward.
            for field in record["fields"].as_array_mut()?.iter_mut() {
                if field["key"] == "estimate" {
                    field["number"] = json!([0]);
                }
            }
            record["fields"].as_array_mut()?.extend([
                json!({"column_id":"C_EDIT_USER","key":"edit_user","user":["U00000001"]}),
                json!({"column_id":"C_EDIT_USERS","key":"edit_users","user":["U00000001","U99999999"]}),
                json!({"column_id":"C_EDIT_TASK_USER","key":"edit_task_user","user":["U00000001"]}),
            ]);
        }
    }
    refresh(shell);
    Some(snapshot)
}
pub(crate) fn user_edit_value(shell: &ShellState, key: &str) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    shell.slack_lists.fixture.as_ref()?.records["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == "R_LIST_000")?["fields"]
        .as_array()?
        .iter()
        .find(|field| field["key"] == key)?
        .get("user")
        .cloned()
}
pub(crate) fn restore_user_edits(shell: &mut ShellState, snapshot: UserEditSnapshot) {
    if !enabled(shell) {
        return;
    }
    if let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get_mut(team))
    {
        workspace.users = snapshot.users;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    refresh(shell);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_people_fixture_isolates_prior_numeric_edits_and_restores_them() {
        let mut shell =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
        let fixture = shell.slack_lists.fixture.as_mut().unwrap();
        let record = fixture.records["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == "R_LIST_000")
            .unwrap();
        record["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["key"] == "estimate")
            .unwrap()["number"] = json!([5]);
        let original = fixture.records.clone();
        let snapshot = add_user_edits(&mut shell).unwrap();
        let view = shell.slack_lists.view.as_ref().unwrap();
        let (visit, layout) = (view.generation, view.layout.generation);
        assert!(super::super::super::layout::select_shell(
            &mut shell,
            Some("V_EDIT_USERS".into()),
            visit,
            layout
        ));
        let row = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.id == "R_LIST_000")
            .expect("prior numeric edit must not hide the User test target");
        assert!(
            row.cells
                .iter()
                .any(|cell| cell.column_id == "C_EDIT_USER" && cell.can_edit)
        );
        assert!(
            row.cells
                .iter()
                .any(|cell| cell.column_id == "C_EDIT_TASK_USER" && !cell.can_edit)
        );
        restore_user_edits(&mut shell, snapshot);
        assert_eq!(
            shell.slack_lists.fixture.as_ref().unwrap().records,
            original
        );
    }
}
