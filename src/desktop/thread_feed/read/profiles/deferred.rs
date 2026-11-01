//! Original profile DATA outside the first visible window, never an opener.
//! A local reveal supplies only visibility; this private roster supplies the
//! independent original profile lineage. Acquisition keeps its existing guards.
use super::*;

#[derive(Clone, Default)]
pub(super) struct Roster {
    rows: BTreeMap<Key, Actor>,
    retired: Arc<Mutex<HashSet<Key>>>,
}

// DATA join only, capped to the existing native200 display budget. No public
// row, count, cache or local-window receipt can insert a profile candidate.
fn kept(shell: &ShellState, team: &str) -> BTreeMap<Key, Actor> {
    let mut rows = BTreeMap::new();
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return rows;
    };
    let mut roots = HashSet::new();
    let mut duplicate = HashSet::new();
    for item in &shell.core.threads_view.items {
        if let Some(key) = item.channel().zip(item.root_ts()) {
            if !roots.insert(key) {
                duplicate.insert(key);
            }
        }
    }
    for item in &shell.core.threads_view.items {
        let (Some(channel), Some(root)) = (item.channel(), item.root_ts()) else {
            continue;
        };
        if duplicate.contains(&(channel, root))
            || item.root_msg.thread_ts.as_ref() != Some(root)
            || item.root_msg.team.as_deref().is_some_and(|v| v != team)
            || item.root_msg.extra.get("subscribed") != Some(&serde_json::json!(true))
            || item.latest_replies.len() + item.unread_replies.len() > 1000
        {
            continue;
        }
        let Some(root_time) = timestamp(root) else {
            continue;
        };
        let channel_bag = workspace.messages.get(channel);
        let thread_bag = shell
            .core
            .threads
            .get(&(team.into(), channel.clone(), root.clone()));
        if channel_bag.is_some_and(|b| b.was_deleted(root))
            || thread_bag.is_some_and(|b| b.was_deleted(root))
        {
            continue;
        }
        // Same timestamp order as the actual feed, no canonical bag union.
        for message in item.replies().into_iter().rev().take(200) {
            let Some(ts) = message.ts.as_ref() else {
                continue;
            };
            if !timestamp(ts).is_some_and(|t| t > root_time)
                || channel_bag.is_some_and(|b| b.was_deleted(ts))
                || thread_bag.is_some_and(|b| b.was_deleted(ts))
            {
                continue;
            }
            if let Some(value) = actor(message, team, channel, root) {
                rows.insert((channel.clone(), root.clone(), ts.clone()), value);
            }
        }
    }
    rows
}
impl Roster {
    pub(super) fn capture(
        shell: &ShellState,
        team: &str,
        previous: Option<&Self>,
        incoming: &BTreeMap<Key, Actor>,
        touched: &BTreeSet<Key>,
        active: &BTreeMap<Key, Actor>,
    ) -> Option<Self> {
        let mut rows = previous.map(|p| p.rows.clone()).unwrap_or_default();
        let mut retired = match previous {
            Some(p) => p.retired.lock().ok()?.clone(),
            None => HashSet::new(),
        };
        for key in touched {
            rows.remove(key);
            retired.remove(key);
        }
        let kept = kept(shell, team);
        // Only new ORIGINAL candidate intake can add deferred data. Untouched
        // earlier data may survive an original native append, never restoration.
        for (key, value) in incoming {
            if !active.contains_key(key) && kept.get(key) == Some(value) {
                rows.insert(key.clone(), value.clone());
            }
        }
        rows.retain(|k, v| !retired.contains(k) && kept.get(k) == Some(v));
        Some(Self {
            rows,
            retired: Arc::new(Mutex::new(retired)),
        })
    }
    pub(super) fn retire_missing(&self, shell: &ShellState, team: &str) {
        if self.rows.is_empty() {
            return;
        }
        let Ok(mut retired) = self.retired.lock() else {
            return;
        };
        let kept = kept(shell, team);
        for (key, value) in &self.rows {
            if kept.get(key) != Some(value) {
                retired.insert(key.clone());
            }
        }
    }
}

pub(super) fn activate_visible(shell: &mut ShellState, key: (&str, &str)) -> bool {
    let Some(scope) = shell.thread_feed_reads.profiles.as_ref().cloned() else {
        return false;
    };
    if !scope.current(shell) {
        return false;
    }
    scope.deferred.retire_missing(shell, &scope.team);
    let Ok(retired) = scope.deferred.retired.lock() else {
        return false;
    };
    let Ok(retired_actors) = scope.retired_actors.lock() else {
        return false;
    };
    let selected = scope.deferred.rows.iter().filter(|(k, value)| {
        k.0 == key.0 && k.1 == key.1 && !retired.contains(*k)
            && !retired_actors.contains(*k)
            // The ORIGINAL visible-row predicate is still required. Keeping
            // deferred data did not make it actionable or authorize a request.
            && current_message(shell, k).is_some_and(|m| actor(m, &scope.team, &k.0, &k.1).as_ref() == Some(*value))
    }).map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>();
    if selected.is_empty() {
        return false;
    }
    drop(retired);
    drop(retired_actors);
    let mut next = scope.as_ref().clone();
    for (key, value) in selected {
        next.deferred.rows.remove(&key);
        next.actors.insert(key, value);
    }
    // Immutable successor retires queued old headers/acquisition publication.
    // Existing member versions (including denied ones) are retained verbatim.
    next.started = Arc::new(AtomicBool::new(false));
    next.acquisition = Arc::new(acquisition::State::default());
    next.retired = Arc::new(AtomicBool::new(false));
    shell.thread_feed_reads.profiles = Some(Arc::new(next));
    true
}
