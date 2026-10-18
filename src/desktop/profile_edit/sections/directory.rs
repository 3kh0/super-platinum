use super::*;
use super_platinum_core::slack::models::User;
#[derive(Clone, Default)]
pub(super) struct Directory {
    query: String,
    generation: u64,
    users: BTreeMap<String, User>,
    results: Vec<String>,
    tags: Vec<String>,
    tag_field: String,
}
pub(crate) fn tags(shell: &ShellState, field: &str, query: &str) -> Vec<String> {
    shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .filter(|d| {
            current(shell, &d.token, d.visit, d.serial)
                && d.directory.tag_field == field
                && d.directory.query == protocol::trim_tag_query(query)
        })
        .map(|d| d.directory.tags.clone())
        .unwrap_or_default()
}
pub(crate) fn query_tags(
    mut state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    field: String,
    query: String,
) {
    let query = protocol::trim_tag_query(&query).to_owned();
    let (token, generation) = {
        let mut shell = state.write();
        if !presentation(&shell)
            .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
        {
            return;
        }
        let d = shell.profile_edit.sections.draft.as_mut().unwrap();
        if !d.form.as_ref().is_some_and(|f| {
            f.fields().iter().any(|f| {
                f.id == field && f.editable && matches!(f.value, Some(SectionValue::Tags(_)))
            })
        }) {
            return;
        }
        d.directory.generation = next_id();
        d.directory.query = query.clone();
        d.directory.tag_field = field.clone();
        d.directory.tags.clear();
        (d.token.clone(), d.directory.generation)
    };
    if query.is_empty() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let ctx = {
            let shell = state.peek();
            if !request_current(&shell, &token, visit, serial, generation)
                || shell
                    .profile_edit
                    .sections
                    .entries
                    .get(&token.owner)
                    .is_some_and(|e| e.retry_at > now())
            {
                return;
            }
            context(&shell, &token)
        };
        let Some(c) = ctx else {
            return;
        };
        let result =
            protocol::fetch_tags(&c.transport, &c.client, &c.session, &field, &query).await;
        let mut shell = state.write();
        if !request_current(&shell, &token, visit, serial, generation) {
            return;
        }
        match result {
            Ok(tags) => {
                shell
                    .profile_edit
                    .sections
                    .draft
                    .as_mut()
                    .unwrap()
                    .directory
                    .tags = tags
            }
            Err(error) => {
                let e = shell
                    .profile_edit
                    .sections
                    .entries
                    .entry(token.owner)
                    .or_default();
                e.retry_at = e.retry_at.max(cooldown(&error));
                e.message = Some(
                    "Tag suggestions could not load. You can retain your draft and try later."
                        .into(),
                );
            }
        }
    });
}
// Captured EditPersonElementForm ef: profile-only users are allowed; bots,
// Slackbot, guests and external users are not. Search is workspace-bound.
pub(super) fn eligible(user: &User) -> bool {
    super_platinum_core::block_actions::form::valid_message_user(&user.id)
        && user.id != "USLACKBOT"
        && !user.deleted
        && !user.is_bot
        && !user.is_restricted
        && !user.is_ultra_restricted
        && !user.is_stranger
        && user.extra.get("is_external") != Some(&serde_json::Value::Bool(true))
}
pub(crate) fn person_name(shell: &ShellState, id: &str) -> String {
    let user = shell.profile_edit.sections.draft.as_ref().and_then(|d| {
        shell
            .core
            .workspaces
            .get(&d.token.owner.team)
            .and_then(|w| w.users.get(id))
            .or_else(|| d.directory.users.get(id))
    });
    super_platinum_core::state::display_name(user, id)
}
pub(crate) fn people(shell: &ShellState, query: &str) -> Vec<(String, String)> {
    let Some(d) = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .filter(|d| current(shell, &d.token, d.visit, d.serial))
    else {
        return Vec::new();
    };
    let Some(workspace) = shell.core.workspaces.get(&d.token.owner.team) else {
        return Vec::new();
    };
    let query = query.trim().to_lowercase();
    let mut people = BTreeMap::new();
    for user in workspace.users.values().filter(|u| eligible(u)) {
        let name = super_platinum_core::state::display_name(Some(user), &user.id);
        if query.is_empty()
            || name.to_lowercase().contains(&query)
            || user
                .name
                .as_ref()
                .is_some_and(|n| n.to_lowercase().contains(&query))
        {
            people.insert(user.id.clone(), name);
        }
    }
    if d.directory.query == query {
        for id in &d.directory.results {
            if let Some(u) = d.directory.users.get(id).filter(|u| eligible(u))
                && workspace.users.get(id).is_none_or(eligible)
            {
                people.insert(
                    id.clone(),
                    super_platinum_core::state::display_name(Some(u), id),
                );
            }
        }
    }
    let mut output: Vec<_> = people.into_iter().collect();
    output.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    output.truncate(30);
    output
}
pub(crate) fn query_people(mut state: Signal<ShellState>, visit: u64, serial: u64, query: String) {
    let query = query.trim().to_lowercase();
    let request = {
        let mut shell = state.write();
        if !presentation(&shell)
            .is_some_and(|v| v.visit == visit && v.serial == serial && !v.busy && !v.uncertain)
        {
            return;
        }
        let d = shell.profile_edit.sections.draft.as_mut().unwrap();
        d.directory.query = query.clone();
        d.directory.generation = next_id();
        d.directory.results.clear();
        (d.token.clone(), d.directory.generation)
    };
    if query.is_empty() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let (token, generation) = request;
        let ctx = {
            let shell = state.peek();
            if !request_current(&shell, &token, visit, serial, generation) {
                return;
            }
            if shell
                .profile_edit
                .sections
                .entries
                .get(&token.owner)
                .is_some_and(|e| e.retry_at > now())
            {
                return;
            }
            context(&shell, &token)
        };
        let Some(c) = ctx else {
            return;
        };
        let result = api::fetch_users_search(&c.transport, &c.client, &c.session, query).await;
        let mut shell = state.write();
        if !request_current(&shell, &token, visit, serial, generation) {
            return;
        }
        match result {
            Ok(users) => {
                let d = shell.profile_edit.sections.draft.as_mut().unwrap();
                // Search results are transient; retain only current selected
                // people and this accepted page, not every query in the visit.
                let selected: std::collections::HashSet<String> = d
                    .form
                    .as_ref()
                    .into_iter()
                    .flat_map(|f| {
                        f.fields()
                            .iter()
                            .filter_map(|field| match f.value(&field.id) {
                                Some(SectionValue::Persons(ids)) => Some(ids.clone()),
                                _ => None,
                            })
                    })
                    .flatten()
                    .collect();
                d.directory.users.retain(|id, _| selected.contains(id));
                for user in users.into_iter().take(30) {
                    d.directory.results.push(user.id.clone());
                    d.directory.users.insert(user.id.clone(), user);
                }
            }
            Err(error) => {
                let e = shell
                    .profile_edit
                    .sections
                    .entries
                    .entry(token.owner)
                    .or_default();
                e.retry_at = e.retry_at.max(cooldown(&error));
                e.message = Some("People search could not load. Wait before trying again.".into());
            }
        }
    });
}
pub(super) fn request_current(
    shell: &ShellState,
    token: &Token,
    visit: u64,
    serial: u64,
    generation: u64,
) -> bool {
    current(shell, token, visit, serial)
        && shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .is_some_and(|d| d.directory.generation == generation)
}
pub(super) fn validates(ids: &[String], users: &[User]) -> bool {
    ids.iter().all(|id| {
        let mut matched = users.iter().filter(|u| &u.id == id);
        matched.next().is_some_and(eligible) && matched.next().is_none()
    })
}
