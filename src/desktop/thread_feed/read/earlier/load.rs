//! One documented full-body read; no permissive HistoryPage defaults/cache merge.
use super::*;
use super_platinum_core::slack::PreparedRequest;
mod pages;

struct Lease {
    action: Action,
    ticket: Arc<Ticket>,
    state: Signal<ShellState>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(_s) = self.state.try_write() {
            self.action.release(&self.ticket, false, None);
        }
    }
}
enum Decoded {
    Complete(Vec<Message>),
    Incomplete,
}
fn invalid() -> Error {
    Error::Api("Unsupported earlier-reply response".into())
}
fn decode(raw: Value, action: &Action) -> Result<Decoded, Error> {
    let seed = &action.scope.windows[&action.key].seed;
    let boundary = seed.ids.first().ok_or_else(invalid)?;
    if raw.get("ok") != Some(&Value::Bool(true)) {
        return Err(invalid());
    }
    let more = raw
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    let cursor = raw
        .get("response_metadata")
        .and_then(|m| m.get("next_cursor"))
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if more || !cursor.is_empty() {
        return Ok(Decoded::Incomplete);
    }
    if raw
        .get("unchanged_messages")
        .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        || ["limited", "is_limited"].iter().any(|k| {
            raw.get(*k)
                .is_some_and(|v| v != &Value::Bool(false) && v != &Value::Null)
        })
    {
        return Err(invalid());
    }
    // Explicit response deletions are denials even if a contradictory full
    // message with that ID is also present. This ordinary slice refuses them.
    if raw.get("deleted").is_some_and(|v| {
        !v.is_null() && v != &Value::Bool(false) && !v.as_array().is_some_and(|a| a.is_empty())
    }) {
        return Err(invalid());
    }
    let messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if messages.len() < 3 || messages.len() > 100 {
        return Err(invalid());
    }
    let mut parsed = BTreeMap::new();
    for raw in messages {
        let message: Message = serde_json::from_value(raw.clone()).map_err(|_| invalid())?;
        let ts = message.ts.clone().ok_or_else(invalid)?;
        if !seed::ordinary(
            &message,
            &action.source.team,
            &action.key,
            ts == action.key.1,
        ) || ts.as_str() > boundary.as_str()
            || parsed.insert(ts, message).is_some()
        {
            return Err(invalid());
        }
    }
    let root = parsed.get(&action.key.1).ok_or_else(invalid)?;
    if root
        .extra
        .get("subscribed")
        .is_some_and(|v| v != &Value::Bool(true))
        || root.reply_count != Some(seed.count)
        || root.latest_reply.as_ref() != Some(&seed.latest)
        || !parsed.contains_key(boundary)
    {
        return Err(invalid());
    }
    let earlier = parsed
        .into_iter()
        .filter_map(|(ts, m)| (ts > action.key.1 && ts < *boundary).then_some(m))
        .collect::<Vec<_>>();
    if earlier.is_empty() || earlier.len() + seed.ids.len() > 200 {
        return Err(invalid());
    }
    Ok(Decoded::Complete(earlier))
}
fn request(action: &Action) -> PreparedRequest {
    request_page(action, None)
}
fn request_page(action: &Action, cursor: Option<String>) -> PreparedRequest {
    let original = &action.scope.source;
    api::conversations_replies(
        &original.client,
        &original.session,
        api::RepliesArgs {
            cursor,
            channel: action.key.0.clone(),
            ts: action.key.1.clone(),
            latest: Some(action.scope.windows[&action.key].seed.ids[0].clone()),
            inclusive: true,
            limit: Some(100),
            ..Default::default()
        },
    )
}
pub(super) async fn run(action: Action, state: Signal<ShellState>, ticket: Arc<Ticket>) {
    #[cfg(test)]
    {
        let io = super::test_io::current();
        run_with(action, state, ticket, move |_transport, request| {
            let io = io.clone();
            async move {
                match io {
                    Some(io) => io.execute(request).await,
                    None => Err(Error::Offline("No scoped earlier-reply fixture IO".into())),
                }
            }
        })
        .await;
    }
    #[cfg(not(test))]
    {
        let dispatch = action.clone();
        let occurrence = ticket.clone();
        run_with(action, state, ticket, move |transport, request| {
            let dispatch = dispatch.clone();
            let occurrence = occurrence.clone();
            async move {
                transport
                    .execute_current(request, || {
                        state
                            .try_peek()
                            .is_ok_and(|s| dispatch.active(&s, &occurrence))
                    })
                    .await
            }
        })
        .await;
    }
}
async fn run_with<F, Fut>(
    action: Action,
    mut state: Signal<ShellState>,
    ticket: Arc<Ticket>,
    mut fetch: F,
) where
    F: FnMut(Arc<Transport>, PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let _lease = Lease {
        action: action.clone(),
        ticket: ticket.clone(),
        state,
    };
    if !state.try_peek().is_ok_and(|s| action.active(&s, &ticket)) {
        return;
    }
    let result = read_pages(&action, state, &ticket, &mut fetch).await;
    let Ok(mut s) = state.try_write() else { return };
    if !action.active(&s, &ticket) {
        return;
    }
    let mut load_profiles = false;
    match result {
        Err(error) => {
            action.release(&ticket, false, None);
            s.report_failure(&error, "Earlier replies need a connection", || {
                "Could not load earlier replies. Please try again.".into()
            });
        }
        Ok(Decoded::Incomplete) => {
            action.release(&ticket, true, None);
            s.show_toast("Open the thread to read more replies.");
        }
        Ok(Decoded::Complete(earlier)) => {
            // Refuse collisions even with ineligible/hidden feed rows or a
            // canonical copy whose version this purpose does not own.
            let team = &action.source.team;
            let key = &action.key;
            let workspace = &s.core.workspaces[team];
            let bag = s
                .core
                .threads
                .get(&(team.clone(), key.0.clone(), key.1.clone()));
            let wanted = earlier
                .iter()
                .filter_map(|m| m.ts.as_deref())
                .collect::<std::collections::HashSet<_>>();
            let occupied = |m: &Message| m.ts.as_deref().is_some_and(|t| wanted.contains(t));
            let collision = workspace
                .messages
                .get(&key.0)
                .is_some_and(|b| b.messages.iter().any(occupied))
                || bag.is_some_and(|b| b.messages.iter().any(occupied))
                || s.core
                    .threads_view
                    .items
                    .iter()
                    .filter(|i| i.root_msg.channel.as_ref() == Some(&key.0))
                    .any(|i| {
                        std::iter::once(&i.root_msg)
                            .chain(&i.latest_replies)
                            .chain(&i.unread_replies)
                            .any(occupied)
                    });
            if collision || wanted.iter().any(|t| seed::deleted(&s, team, key, t)) {
                action.release(&ticket, true, None);
                s.show_toast("Open the thread to read more replies.");
                return;
            }
            drop(wanted);
            let Some(index) = s.core.threads_view.items.iter().position(|i| {
                i.root_msg.channel.as_ref() == Some(&key.0)
                    && i.root_msg.ts.as_ref() == Some(&key.1)
            }) else {
                return;
            };
            let profile_candidates =
                profiles::network::Candidates::from_replies(&earlier, team, key);
            let mut merged = earlier;
            // Existing current bodies win unconditionally; root and inclusive
            // boundary response bodies never replace the displayed copies.
            merged.extend(s.core.threads_view.items[index].latest_replies.clone());
            merged.sort_by(|a, b| a.ts.cmp(&b.ts));
            let ids = merged
                .iter()
                .filter_map(|m| m.ts.clone())
                .collect::<Vec<_>>();
            if ids.len() > 200 || ids.windows(2).any(|v| v[0] >= v[1]) {
                return;
            }
            // Linearize the bounded publication against consumed raw ingress.
            // Account/page/current-row checks above share this serial Shell
            // borrow. No registry query, IO or await occurs inside this lock.
            let published = ticket
                .watch
                .with_current(|| {
                    if !action.release(&ticket, false, Some(ids.clone())) {
                        return None;
                    }
                    let previous = std::mem::replace(
                        &mut s.core.threads_view.items[index].latest_replies,
                        merged,
                    );
                    Some((
                        Published {
                            action: action.clone(),
                            ids,
                        },
                        previous,
                    ))
                })
                .flatten();
            if let Some((published, previous)) = published {
                drop(previous); // Body deallocation stays outside the ingress lock.
                let profile_intake = ProfileIntake {
                    action: published.action.clone(),
                    ticket: ticket.clone(),
                    ids: published.ids.clone(),
                    candidates: profile_candidates,
                };
                windows::publish_earlier_context(&mut s, published);
                load_profiles = profiles::network::publish(&mut s, profile_intake);
                // No newly read row acquires an OLD local expansion/profile/
                // row-action binding. The separate raw actor intake above is
                // still unusable without its own accepted human-member read.
                super::super::retire_after_native_mutation(&s);
            }
        }
    }
    drop(s);
    // The completed body read must release its watch BEFORE member IO. Both
    // the production fetch closure and lease hold the original ticket too.
    drop(fetch);
    drop(ticket);
    drop(_lease);
    if load_profiles {
        profiles::load(action.source.clone(), state).await;
    }
}

// The same original ticket/watch spans every await; no page can rearm it or
// create a new purpose from a cursor. No partial result enters visible state.
async fn read_pages<F, Fut>(
    action: &Action,
    state: Signal<ShellState>,
    ticket: &Arc<Ticket>,
    fetch: &mut F,
) -> Result<Decoded, Error>
where
    F: FnMut(Arc<Transport>, PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let current = || state.try_peek().is_ok_and(|s| action.active(&s, ticket));
    let superseded = || Error::Api("request_superseded".into());
    if !current() {
        return Err(superseded());
    }
    let raw = fetch(action.scope.source.transport.clone(), request(action)).await?;
    if !current() {
        return Err(superseded());
    }
    let Some(mut collector) = pages::Collector::start(&raw, action)? else {
        return decode(raw, action);
    };
    drop(raw); // Do not retain the first raw payload across later awaits.
    loop {
        if !current() {
            return Err(superseded());
        }
        let raw = fetch(
            action.scope.source.transport.clone(),
            request_page(action, Some(collector.cursor())),
        )
        .await?;
        if !current() {
            return Err(superseded());
        }
        match collector.accept(raw, action)? {
            pages::Step::Next => {}
            pages::Step::Complete(earlier) => return Ok(Decoded::Complete(earlier)),
            pages::Step::Fallback => return Ok(Decoded::Incomplete),
        }
    }
}
