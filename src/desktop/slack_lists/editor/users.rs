//! Source-backed ordinary User-column search candidates.
use super::entities::{LIMIT, workspace};
use super::*;
use super_platinum_core::state::Workspace;
use super_platinum_core::{slack::models::User, state::display_name};
pub(super) fn selected_labels(shell: &ShellState, ids: &[String]) -> Vec<EditorChoice> {
    ids.iter()
        .map(|id| EditorChoice {
            id: id.clone(),
            label: workspace(shell)
                .map_or_else(|| id.clone(), |workspace| workspace.display_name(id)),
        })
        .collect()
}
fn eligible(user: &User) -> bool {
    user.id.len() >= 9
        && matches!(user.id.as_bytes().first(), Some(b'U' | b'W'))
        && user
            .id
            .bytes()
            .skip(1)
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && !user.deleted
        && !user.is_bot
        && user.id != "USLACKBOT"
        && user
            .extra
            .get("is_profile_only_user")
            .is_none_or(|value| value != &Value::Bool(true))
}
pub(super) fn candidates<'a>(
    workspace: &Workspace,
    users: impl Iterator<Item = (&'a User, bool)>,
    query: &str,
) -> Vec<EditorChoice> {
    let query = query.trim().trim_start_matches('@').to_lowercase();
    let mut scores = BTreeMap::<String, f64>::new();
    let timestamp = now();
    for channel in workspace.channels.values().filter(|channel| channel.is_im) {
        if let Some(user) = &channel.user {
            let score = workspace.frecency_score(&channel.id, timestamp);
            scores
                .entry(user.clone())
                .and_modify(|old| *old = old.max(score))
                .or_insert(score);
        }
    }
    // Keep only the best thirty, rather than sorting/cloning the directory.
    type Ranked = (bool, u8, f64, String, EditorChoice);
    let compare = |a: &Ranked, b: &Ranked| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(b.2.total_cmp(&a.2))
            .then(a.3.cmp(&b.3))
            .then(a.4.id.cmp(&b.4.id))
    };
    let mut best: Vec<Ranked> = Vec::with_capacity(LIMIT + 1);
    for (user, remote_match) in users {
        if !eligible(&user)
            || workspace
                .users
                .get(&user.id)
                .is_some_and(|cached| !eligible(cached))
        {
            continue;
        }
        let name = display_name(Some(&user), &user.id);
        let folded = name.to_lowercase();
        let handle = user.name.as_deref().unwrap_or_default().to_lowercase();
        let real = user
            .real_name
            .as_deref()
            .or_else(|| {
                user.profile
                    .as_ref()
                    .and_then(|profile| profile.real_name.as_deref())
            })
            .unwrap_or_default()
            .to_lowercase();
        let email = user
            .profile
            .as_ref()
            .and_then(|profile| profile.email.as_deref())
            .unwrap_or_default()
            .to_lowercase();
        if !remote_match
            && !query.is_empty()
            && ![&folded, &handle, &real, &email]
                .iter()
                .any(|value| value.contains(&query))
        {
            continue;
        }
        if best.iter().any(|entry| entry.4.id == user.id) {
            continue;
        }
        let quality = if folded == query || handle == query {
            0
        } else if folded.starts_with(&query) || handle.starts_with(&query) {
            1
        } else {
            2
        };
        let entry = (
            user.id != workspace.self_user_id,
            quality,
            scores.get(&user.id).copied().unwrap_or(0.0),
            folded,
            EditorChoice {
                id: user.id.clone(),
                label: name,
            },
        );
        let index = best.partition_point(|current| compare(current, &entry).is_lt());
        if index < LIMIT {
            best.insert(index, entry);
            best.truncate(LIMIT);
        }
    }
    best.into_iter().map(|entry| entry.4).collect()
}
