//! Only the owned People fixture directory. No network/user data is consulted.
use super::*;
pub(crate) fn seed(shell: &mut ShellState) -> Result<(), String> {
    if !super::super::super::enabled(shell)
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        return Err("Disconnected People seed required".into());
    }
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing workspace")?;
    let mut raw = vec![
        json!({"id":"U00000001","name":"Alice Example","real_name":"Alice Example","profile":{"display_name":"Alice Example"}}),
        json!({"id":"U00000002","name":"Bob Guest","real_name":"Bob Guest","profile":{"display_name":"Bob Guest"},"is_restricted":true}),
    ];
    for i in 3..=11 {
        let name = format!("Person {i:02}");
        raw.push(json!({"id":format!("U{i:08}"),"name":name,"real_name":name,"profile":{"display_name":name}}));
    }
    for (id, name, extra) in [
        ("W00000012", "Whitney External", json!({"is_stranger":true})),
        ("U00000013", "Automation Bot", json!({"is_bot":true})),
        ("U00000014", "Deleted Person", json!({"deleted":true})),
        (
            "U00000015",
            "Profile Only",
            json!({"is_profile_only_user":true}),
        ),
        ("USLACKBOT", "Slackbot", json!({})),
    ] {
        let mut v = json!({"id":id,"name":name,"real_name":name,"profile":{"display_name":name}});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        raw.push(v);
    }
    let users = raw
        .into_iter()
        .map(|v| {
            serde_json::from_value::<super_platinum_core::slack::models::User>(v)
                .map(|u| (u.id.clone(), u))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    if users.len() != 16
        || users.contains_key(&workspace.self_user_id)
        || workspace
            .channels
            .values()
            .any(|c| c.is_im && c.user.as_ref().is_some_and(|id| users.contains_key(id)))
    {
        return Err("People fixture has nonneutral directory priority".into());
    }
    workspace.users = users;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Live People cache forbidden".into());
    }
    let team = shell.core.active_team.as_ref().ok_or("Missing team")?;
    let w = shell.core.workspaces.get(team).ok_or("Missing workspace")?;
    if w.users.len() != 16 {
        return Err("People fixture cache inventory changed".into());
    }
    Ok(json!({"team":team,"self":w.self_user_id,"users":w.users,"channels":w.channels}))
}
