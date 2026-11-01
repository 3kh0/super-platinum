//! Earlier-response actor intake, independent of original getView candidates.
//! A public row, reading-memory token or cached user cannot create this grant.
use super::*;

pub(super) type Bindings = BTreeMap<Key, Arc<super::super::earlier::ProfileLineage>>;

// iOp7's raw msg.user identity branch only. Special author overrides are not
// inferred from their fallback user, a file sender or a successful member read.
pub(super) fn ordinary(message: &Message) -> bool {
    message.message.is_none()
        && message.files.is_empty()
        && message.user.as_deref() != Some("USLACKBOT")
        && ![
            "document_comment",
            "comment",
            "file",
            "ai_context",
            "list_edit",
            "room_id",
            "tab_added_user_id",
            "item_added_user_id",
            "app_id",
        ]
        .iter()
        .any(|key| message.extra.contains_key(*key))
}

pub(in crate::thread_feed::read) struct Candidates {
    team: String,
    thread: (String, String),
    rows: BTreeMap<Key, Actor>,
}
impl Candidates {
    // Data captured from decoded actual response bodies BEFORE the feed merge.
    // It has no publication constructor; the separate sealed IO proof is needed.
    pub(in crate::thread_feed::read) fn from_replies(
        replies: &[Message],
        team: &str,
        thread: &(String, String),
    ) -> Self {
        let mut rows = BTreeMap::new();
        if replies.len() <= 200 {
            for message in replies {
                if !ordinary(message) {
                    continue;
                }
                if let (Some(ts), Some(value)) = (
                    message.ts.as_ref(),
                    actor(message, team, &thread.0, &thread.1),
                ) {
                    rows.insert((thread.0.clone(), thread.1.clone(), ts.clone()), value);
                }
            }
        }
        Self {
            team: team.into(),
            thread: thread.clone(),
            rows,
        }
    }
    pub(in crate::thread_feed::read) fn matches(
        &self,
        team: &str,
        thread: &(String, String),
    ) -> bool {
        self.team == team && &self.thread == thread
    }
}

pub(in crate::thread_feed::read) fn publish(
    shell: &mut ShellState,
    proof: super::super::earlier::ProfileIntake,
) -> bool {
    let Some((source, candidates, lineage)) = proof.details(shell) else {
        return false;
    };
    let Some(scope) = shell.thread_feed_reads.profiles.as_ref().cloned() else {
        return false;
    };
    if !scope.current(shell)
        || !source.current(shell)
        || source.team != scope.team
        || !Arc::ptr_eq(&source.root, &scope.origin)
        || !source.client.same_configuration(&scope.client)
    {
        return false;
    }
    let Ok(retired) = scope.retired_actors.lock() else {
        return false;
    };
    let added = candidates
        .rows
        .into_iter()
        .filter(|(key, value)| {
            !scope.actors.contains_key(key)
                && !retired.contains(key)
                && current_message(shell, key).is_some_and(|m| {
                    ordinary(m) && actor(m, &scope.team, &key.0, &key.1).as_ref() == Some(value)
                })
        })
        .collect::<Vec<_>>();
    if added.is_empty() {
        return false;
    }
    drop(retired);
    let mut next = scope.as_ref().clone();
    for (key, value) in added {
        next.network.insert(key.clone(), lineage.clone());
        next.actors.insert(key, value);
    }
    // Keep exact existing member versions/denials and retirement latches. New
    // missing actors use the original strict serial100 member runner. Headers
    // remain unavailable until a genuine accepted human-member response exists.
    next.started = Arc::new(AtomicBool::new(false));
    next.acquisition = Arc::new(acquisition::State::default());
    next.retired = Arc::new(AtomicBool::new(false));
    shell.thread_feed_reads.profiles = Some(Arc::new(next));
    true
}
