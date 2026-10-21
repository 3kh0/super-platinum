use super::*;
use super_platinum_core::slack::api;

pub(in crate::slack_lists::comments) struct Observation {
    pub id: u64,
    pub revision: u64,
    pub raw: Option<Value>,
    pub deadline: i64,
}
impl Observation {
    pub(super) fn capture(id: u64, revision: u64, result: Result<Value, Error>) -> Self {
        let deadline = match &result {
            Err(error) => failure_deadline(error),
            Ok(raw) if raw["error"] == "ratelimited" => now().saturating_add(30),
            _ => 0,
        };
        Self {
            id,
            revision,
            raw: result.ok(),
            deadline,
        }
    }
    fn stops_batch(&self) -> bool {
        self.deadline > now()
    }
}
/// Explicit full refresh only; immutable receipt bound to original action/read.
pub(in crate::slack_lists::comments) async fn readback(
    state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    page: &Page,
) -> Vec<Observation> {
    let pending = {
        let shell = state.peek();
        let Ok(actions) = shell.slack_lists.comments.reaction_actions.0.lock() else {
            return vec![];
        };
        actions
            .iter()
            .filter(|a| a.belongs(owner, page.target()) && a.phase == Phase::Unknown)
            .filter_map(|a| {
                a.record
                    .as_ref()
                    .filter(|r| r.observed(page) != Ok(true))
                    .map(|r| (a.id, r.clone(), a.socket_revision))
            })
            .collect::<Vec<_>>()
    };
    // One authenticated full response can prove several original action names
    // on the same exact message; capture every socket revision before the read.
    let mut groups: Vec<Vec<(u64, ReactionRecord, u64)>> = Vec::new();
    for item in pending {
        if let Some(group) = groups.iter_mut().find(|g| {
            g[0].1.target().channel_id() == item.1.target().channel_id()
                && g[0].1.ts() == item.1.ts()
        }) {
            group.push(item);
        } else {
            groups.push(vec![item]);
        }
    }
    let mut receipts = Vec::new();
    for group in groups {
        let (id, record, revision) = &group[0];
        let input = {
            let shell = state.peek();
            if !owner.reading(&shell, serial) {
                return vec![];
            }
            if crate::fixture::is_fixture() {
                if !fixture::enabled(&shell) {
                    return vec![];
                }
                crate::slack_lists::comments::fixture::compose::session(&shell)
                    .ok()
                    .map(|w| (shell.core.client.clone(), w))
            } else {
                owner.scope.request(&shell).map(|c| (c.client, c.session))
            }
        };
        let Some((client, session)) = input else {
            return vec![];
        };
        let request = api::reactions_get(
            &client,
            &session,
            record.target().channel_id().into(),
            record.ts().into(),
        );
        let result = network::request(state, owner, serial, request, false).await;
        let receipt = Observation::capture(*id, *revision, result);
        let stop = receipt.stops_batch();
        for (id, _, revision) in group {
            receipts.push(Observation {
                id,
                revision,
                raw: receipt.raw.clone(),
                deadline: receipt.deadline,
            });
        }
        if stop {
            break;
        }
    }
    receipts
}
pub(in crate::slack_lists::comments) fn readback_current(
    shell: &ShellState,
    receipts: &[Observation],
) -> bool {
    shell
        .slack_lists
        .comments
        .reaction_actions
        .0
        .lock()
        .is_ok_and(|actions| {
            receipts.iter().all(|r| {
                actions
                    .iter()
                    .any(|a| a.id == r.id && a.socket_revision == r.revision)
            })
        })
}
pub(in crate::slack_lists::comments) fn apply_readback(
    shell: &mut ShellState,
    owner: &Owner,
    receipts: &[Observation],
) {
    if !owner.active(shell) {
        return;
    }
    let deadline = receipts.iter().map(|r| r.deadline).max().unwrap_or(0);
    if deadline > now() {
        let deadline = deadline.max(cooldown(shell));
        shell.slack_lists.comments.cooldown = Some((owner.scope.receipt_only(), deadline));
        if let Some(e) = shell.slack_lists.comments.entry.as_mut() {
            e.retry_at = e.retry_at.max(deadline);
        }
    }
    if !readback_current(shell, receipts) {
        return;
    }
    for observed in receipts {
        let Some(raw) = &observed.raw else {
            continue;
        };
        let action = {
            let Ok(actions) = shell.slack_lists.comments.reaction_actions.0.lock() else {
                return;
            };
            actions
                .iter()
                .find(|a| a.id == observed.id && a.phase == Phase::Unknown)
                .cloned()
        };
        let Some(action) = action else {
            continue;
        };
        let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
            return;
        };
        if !action.belongs(owner, &entry.target)
            || entry.deleted.contains(&action.ts)
            || !entry.rows.iter().any(|r| r.ts == action.ts)
        {
            continue;
        }
        let Some(record) = &action.record else {
            continue;
        };
        let full = record.full_reactions(raw);
        if let Ok(Some(reactions)) = full {
            let (Some(mut page), Some(projection)) =
                (entry.page.clone(), entry.projection.as_ref())
            else {
                continue;
            };
            let mut accepted = entry.accepted.clone();
            let mut live = entry.live.clone();
            page.replace_reply_reactions(entry.target.channel_id(), &action.ts, &reactions);
            for row in accepted
                .iter_mut()
                .chain(&mut live)
                .filter(|m| m.ts.as_deref() == Some(action.ts.as_str()))
            {
                row.reactions = reactions.clone();
            }
            let Some(rows) =
                accepted::rows_with_live(shell, owner, &page, projection, &accepted, &live)
            else {
                continue;
            };
            let e = shell.slack_lists.comments.entry.as_mut().unwrap();
            e.page = Some(page);
            e.accepted = accepted;
            e.live = live;
            e.rows = rows;
            // A later local comment ACK must preserve this exact full snapshot.
            if e.live
                .iter()
                .any(|m| m.ts.as_deref() == Some(action.ts.as_str()))
            {
                e.live_reactions.insert(action.ts.clone());
            }
        }
        if let Ok(mut actions) = shell.slack_lists.comments.reaction_actions.0.lock() {
            if let Some(a) = actions
                .iter_mut()
                .find(|a| a.id == action.id && a.phase == Phase::Unknown)
            {
                a.full_readback = Some(raw.clone());
                if record.observed_full(raw) == Ok(true) {
                    a.phase = Phase::Observed;
                    a.readback = true;
                }
            }
        }
    }
}
