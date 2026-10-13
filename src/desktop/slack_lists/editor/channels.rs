//! Channel references do not join, share, or change channel membership.
use super::entities::{Choice, LIMIT, workspace};
use super::*;
use super_platinum_core::slack::models::Channel;
use super_platinum_core::state::Workspace;

pub(super) fn selected_labels(shell: &ShellState, ids: &[String]) -> Vec<Choice> {
    ids.iter()
        .map(|id| Choice {
            id: id.clone(),
            label: workspace(shell)
                .and_then(|workspace| workspace.channels.get(id))
                .and_then(|channel| channel.name.as_deref())
                .filter(|name| !name.is_empty())
                .map_or_else(|| id.clone(), |name| format!("#{name}")),
        })
        .collect()
}
fn eligible(channel: &Channel) -> bool {
    channel.id.len() >= 9
        && matches!(
            channel.id.as_bytes().first(),
            Some(b'C' | b'G' | b'D' | b'Z')
        )
        && channel
            .id
            .bytes()
            .skip(1)
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && (channel.is_channel || channel.is_private || channel.is_group)
        && !channel.is_archived
        && !channel.is_im
        && !channel.is_mpim
        && channel.name.as_deref().is_some_and(|name| !name.is_empty())
        && (!(channel.is_private || channel.is_group)
            || !special_record(channel)
            || channel.extra.get("is_member") == Some(&Value::Bool(true)))
}
pub(super) fn candidates<'a>(
    workspace: &Workspace,
    channels: impl Iterator<Item = (&'a Channel, bool)>,
    query: &str,
) -> Vec<Choice> {
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let timestamp = now();
    type Ranked = (u8, f64, String, Choice);
    let compare = |a: &Ranked, b: &Ranked| {
        a.0.cmp(&b.0)
            .then(b.1.total_cmp(&a.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.id.cmp(&b.3.id))
    };
    let mut best: Vec<Ranked> = Vec::with_capacity(LIMIT + 1);
    for (channel, remote_match) in channels {
        if !eligible(channel)
            || workspace
                .channels
                .get(&channel.id)
                .is_some_and(|cached| !eligible(cached))
        {
            continue;
        }
        let name = channel.name.as_deref().unwrap();
        let folded = name.to_lowercase();
        if !remote_match && !folded.contains(&query) {
            continue;
        }
        if best.iter().any(|entry| entry.3.id == channel.id) {
            continue;
        }
        let quality = if folded == query {
            0
        } else if folded.starts_with(&query) {
            1
        } else {
            2
        };
        let entry = (
            quality,
            workspace.frecency_score(&channel.id, timestamp),
            folded,
            Choice {
                id: channel.id.clone(),
                label: format!("#{name}"),
            },
        );
        let index = best.partition_point(|current| compare(current, &entry).is_lt());
        if index < LIMIT {
            best.insert(index, entry);
            best.truncate(LIMIT);
        }
    }
    best.into_iter().map(|entry| entry.3).collect()
}

fn special_record(channel: &Channel) -> bool {
    channel
        .extra
        .get("properties")
        .and_then(|properties| properties.pointer("/record_channel/record_id"))
        .and_then(Value::as_str)
        .is_some_and(|id| {
            let parts = id.split(':').collect::<Vec<_>>();
            parts.len() == 3 && parts[0] == "S"
        })
}
