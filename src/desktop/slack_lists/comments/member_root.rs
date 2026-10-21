//! Original List thread-root facts for member completion, separate from the
//! visible reply page. Pagination never synthesizes participants from authors.
use super::*;
use serde_json::json;
use super_platinum_core::slack::events::{MessageUpdateKind, RtEvent};

#[derive(Clone, Default)]
pub(super) struct State {
    observed: Option<Source>,
}
#[derive(Clone)]
struct Source {
    owner: Owner,
    target: Target,
    socket: u64,
    // None is an observed missing root from an accepted Page, not an empty cache.
    root: Option<Value>,
}
impl Source {
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.active(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.scope.team)
                .is_some_and(|w| w.rt_generation == self.socket)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.target == self.target)
    }
}

fn thread_users(root: Option<&Value>) -> Result<Vec<Value>, Error> {
    match root.and_then(|r| r.get("reply_users")) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(users))
            if users.iter().all(|user| {
                user.as_str().is_some_and(|id| {
                    !id.is_empty() && !id.chars().any(|c| c.is_control() || c.is_whitespace())
                })
            }) =>
        {
            Ok(users.clone())
        }
        _ => Err(unavailable()),
    }
}

pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, Error> {
    let source = shell
        .slack_lists
        .comments
        .member_root
        .observed
        .as_ref()
        .filter(|s| s.current(shell))
        .ok_or_else(unavailable)?;
    let users = thread_users(source.root.as_ref())?;
    Ok(
        json!({"channelId":source.target.channel_id(),"rootTs":source.target.root_ts(),
        "root":source.root,"threadUsers":users}),
    )
}

/// Called only in the accepted current read commit, after projection succeeds.
pub(super) fn accepted_page(shell: &mut ShellState, owner: &Owner, serial: u64, page: &Page) {
    if !owner.reading(shell, serial) {
        return;
    }
    let Some(socket) = shell
        .core
        .workspaces
        .get(&owner.scope.team)
        .map(|w| w.rt_generation)
    else {
        return;
    };
    let root = page
        .root()
        .and_then(|message| message.ts.as_deref())
        .and_then(|ts| {
            page.raw()["messages"]
                .as_array()?
                .iter()
                .find(|r| r["ts"].as_str() == Some(ts))
                .cloned()
        });
    let store = &mut shell.slack_lists.comments.member_root;
    // A root omission carries no replacement or deletion evidence. Keep the
    // exact known root only within this original owner/target/socket lifetime.
    let retained = store
        .observed
        .as_ref()
        .filter(|previous| {
            previous.owner.serial == owner.serial
                && previous.target == *page.target()
                && previous.socket == socket
        })
        .and_then(|previous| previous.root.clone());
    store.observed = Some(Source {
        owner: owner.clone(),
        target: page.target().clone(),
        socket,
        root: root.or(retained),
    });
}

/// Partial socket roots carry only their present fields. They never rewrite the
/// immutable HTTP Page, visible reply rows, or ordinary reply-edit custody.
pub(crate) fn apply(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let RtEvent::MessageChanged {
        channel,
        message,
        original_update: Some(update),
    } = event
    else {
        return false;
    };
    let Some(source) = shell.slack_lists.comments.member_root.observed.as_ref() else {
        return false;
    };
    let Some(root_ts) = source.target.root_ts() else {
        return false;
    };
    if !source.current(shell)
        || source.owner.scope.team != team
        || source.socket != generation
        || shell.core.active_team.as_deref() != Some(team)
        || channel != source.target.channel_id()
        || message.ts.as_deref() != Some(root_ts)
        || !matches!(
            update.kind(),
            MessageUpdateKind::Changed | MessageUpdateKind::Replied
        )
        || !update.matches_identity(channel, message)
    {
        return false;
    }
    let Some(raw) = update.raw().as_object() else {
        return false;
    };
    let Some(users) = raw.get("reply_users") else {
        return false;
    };
    if raw.get("ts").and_then(Value::as_str) != Some(root_ts)
        || raw
            .get("channel")
            .is_some_and(|v| v.as_str() != Some(channel.as_str()))
        || raw
            .get("thread_ts")
            .is_some_and(|v| !v.is_null() && v.as_str() != Some(root_ts))
        || raw
            .get("subtype")
            .is_some_and(|v| v.as_str() != Some("list_record_comment"))
        || raw.get("slack_list").is_some_and(|v| {
            v.as_object().is_none()
                || v.get("list_id")
                    .is_some_and(|id| id.as_str() != Some(source.target.list_id()))
                || v.get("list_record_id")
                    .is_some_and(|id| id.as_str() != Some(source.target.record_id()))
        })
    {
        return false;
    }
    let Some(previous) = source.root.as_ref().and_then(Value::as_object) else {
        return false;
    };
    let mut next = previous.clone();
    // Only participant evidence belongs to this source. A root body edit must
    // not change ordinary comment-row behavior or cancel a page read.
    next.insert("reply_users".into(), users.clone());
    if &next == previous {
        return false;
    }
    // Keep malformed original evidence as unavailable; do not authorize stale
    // participants until a subsequent accepted root repairs the source.
    shell
        .slack_lists
        .comments
        .member_root
        .observed
        .as_mut()
        .unwrap()
        .root = Some(Value::Object(next));
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    if entry.loading && entry.read_replaces_page {
        entry.read = next_id();
        entry.loading = false;
        entry.error = Some(
            "Newer comment updates arrived. Loaded comments are retained. Refresh to try again."
                .into(),
        );
    }
    true
}

#[cfg(test)]
mod tests;
