use super::*;
use super_platinum_core::{
    slack::api,
    slack_lists::comments::{ReplyQuery, Tombstones},
    state::posting_metadata::authority::{Authority, RawChannel},
};

enum Witness {
    Live(Authority, RawChannel),
    Fixture,
}
impl Witness {
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Live(authority, channel) => {
                channel.current(&shell.core) && authority.metadata(&shell.core).is_ok()
            }
            Self::Fixture => fixture::enabled(shell),
        }
    }
}
fn inputs(
    shell: &ShellState,
    work: &Work,
) -> Result<
    (
        super_platinum_core::slack::SlackClient,
        super_platinum_core::config::WorkspaceSession,
    ),
    Error,
> {
    if !work.current(shell) {
        return Err(unavailable());
    }
    if crate::fixture::is_fixture() {
        if !fixture::enabled(shell) {
            return Err(unavailable());
        }
        return Ok((
            shell.core.client.clone(),
            super::super::fixture::compose::session(shell)?,
        ));
    }
    let context = work.owner.scope.request(shell).ok_or_else(unavailable)?;
    Ok((context.client, context.session))
}
async fn prepare(
    mut state: Signal<ShellState>,
    work: &Work,
) -> Result<(Vec<(Work, ReactionIntent)>, Witness), Error> {
    let loaded = network::acquire(state, &work.owner, work.serial, &work.target, None).await?;
    if !same_target(loaded.page.target(), &work.target) {
        return Err(unavailable());
    }
    let root = loaded.page.root().ok_or_else(unavailable)?.clone();
    let page = if loaded
        .page
        .messages()
        .iter()
        .any(|m| m.ts.as_deref() == Some(work.ts.as_str()))
    {
        loaded.page.clone()
    } else {
        let (client, session) = inputs(&state.peek(), work)?;
        let query = ReplyQuery {
            oldest: Some(work.ts.clone()),
            ..Default::default()
        };
        let request = api::list_record_comments_replies(
            &client,
            &session,
            loaded.page.target(),
            &query,
            Tombstones::Include,
        )?;
        network::reply_page(
            state,
            &work.owner,
            work.serial,
            request,
            loaded.page.target(),
        )
        .await?
    };
    let selected = page
        .messages()
        .iter()
        .find(|m| m.ts.as_deref() == Some(work.ts.as_str()))
        .ok_or_else(unavailable)?;
    if loaded
        .projection
        .visible(
            std::slice::from_ref(selected),
            work.target.root_ts().unwrap_or(""),
        )
        .is_empty()
        || page.raw()["deleted"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(work.ts.as_str())))
    {
        return Err(unavailable());
    }
    let lease = draft::seal_for_render(state).await?;
    let read = {
        let mut shell = state.write();
        if !work.current(&shell) || !work.owner.reading(&shell, work.serial) {
            return Err(unavailable());
        }
        if crate::fixture::is_fixture() {
            None
        } else {
            let authority = Authority::capture(&shell.core, &work.owner.scope.team)
                .map_err(|_| unavailable())?;
            let read = authority
                .begin_channel_read(&mut shell.core, work.target.channel_id())
                .map_err(|_| unavailable())?;
            Some((authority, read))
        }
    };
    if let Some(lease) = lease {
        lease.release().await?;
    }
    let (client, session) = inputs(&state.peek(), work)?;
    let request = api::conversations_generic_info(
        &client,
        &session,
        &BTreeMap::from([(work.target.channel_id().into(), 0)]),
    );
    let raw = network::request(state, &work.owner, work.serial, request, false).await?;
    if raw["ok"] != true {
        return Err(unavailable());
    }
    let rows = raw["channels"].as_array().ok_or_else(unavailable)?;
    let mut rows = rows
        .iter()
        .filter(|v| v["id"].as_str() == Some(work.target.channel_id()));
    let channel = rows.next().ok_or_else(unavailable)?.clone();
    if rows.next().is_some() {
        return Err(unavailable());
    }
    let mut intents = if let Some(f) = &work.family {
        let mut intent = ReactionFamilyIntent::capture(
            loaded.page.target(),
            &root,
            selected,
            channel.clone(),
            &work.user,
            &f.selected,
        )
        .map_err(|e| Error::Api(e.to_string()))?;
        if !intent
            .record()
            .children()
            .iter()
            .map(|r| r.name())
            .eq(f.names.iter().map(String::as_str))
        {
            return Err(unavailable());
        }
        let mut children = Vec::new();
        while let Some(child) = intent.take_next() {
            children.push(child);
        }
        children
    } else if let Some(choice) = &work.choice {
        if !choice.matches_fresh(&state.peek(), selected) {
            return Err(unavailable());
        }
        vec![
            ReactionIntent::capture_choice(
                loaded.page.target(),
                &root,
                selected,
                channel.clone(),
                &work.user,
                choice.intent.clone(),
            )
            .map_err(|e| Error::Api(e.to_string()))?,
        ]
    } else {
        vec![
            ReactionIntent::capture(
                loaded.page.target(),
                &root,
                selected,
                channel.clone(),
                &work.user,
                &work.name,
            )
            .map_err(|e| Error::Api(e.to_string()))?,
        ]
    };
    if intents.iter().any(|i| i.record().adding() != work.adding) {
        return Err(unavailable());
    }
    let lease = draft::seal_for_render(state).await?;
    let (children, witness) = {
        let mut shell = state.write();
        if !work.current(&shell) || !work.owner.reading(&shell, work.serial) {
            return Err(unavailable());
        }
        let witness = if let Some((authority, read)) = read {
            Witness::Live(
                authority,
                read.apply(&mut shell.core, channel)
                    .map_err(|_| unavailable())?,
            )
        } else {
            Witness::Fixture
        };
        let children = if work.family.is_some() {
            family::children(work, &intents).ok_or_else(unavailable)?
        } else {
            work.update(|a| {
                a.record = Some(intents[0].record().clone());
                a.phase = Phase::Sent;
            });
            vec![work.clone()]
        };
        shell.slack_lists.comments.entry.as_mut().unwrap().loading = false;
        (children, witness)
    };
    if let Some(lease) = lease {
        lease.release().await?;
    }
    Ok((
        children.into_iter().zip(intents.drain(..)).collect(),
        witness,
    ))
}
pub(super) async fn toggle(
    mut state: Signal<ShellState>,
    generation: u64,
    ts: String,
    name: String,
) {
    let Ok(lease) = draft::seal_for_render(state).await else {
        return;
    };
    let Some(work) = begin(&mut state.write(), generation, &ts, &name) else {
        return;
    };
    if let Some(lease) = lease {
        if lease.release().await.is_err() {
            settle(
                &work,
                Err(ListCommentReactionFailure::NotRequested(unavailable())),
            );
            family::stop(&work);
            retire_preflight(&mut state.write(), &work);
            return;
        }
    }
    execute(state, work).await;
}
pub(super) async fn execute(mut state: Signal<ShellState>, work: Work) {
    let (batch, witness) = match prepare(state, &work).await {
        Ok(ready) => ready,
        Err(error) => {
            let deadline =
                settle_result(&work, Err(ListCommentReactionFailure::NotRequested(error)));
            family::stop(&work);
            publish(state, &work, deadline).await;
            return;
        }
    };
    for (index, (child, intent)) in batch.into_iter().enumerate() {
        if index > 0 {
            let Ok(lease) = draft::seal_for_render(state).await else {
                family::stop(&work);
                retire_preflight(&mut state.write(), &work);
                return;
            };
            let allowed = {
                let shell = state.peek();
                child.current(&shell) && witness.current(&shell) && cooldown(&shell) <= now()
            };
            if !allowed {
                family::stop(&work);
                drop(state.write());
                if let Some(lease) = lease {
                    let _ = lease.release().await;
                }
                return;
            }
            child.update(|a| a.phase = Phase::Sent);
            drop(state.write());
            if let Some(lease) = lease {
                if lease.release().await.is_err() {
                    settle(
                        &child,
                        Err(ListCommentReactionFailure::NotRequested(unavailable())),
                    );
                    family::stop(&work);
                    retire_preflight(&mut state.write(), &work);
                    return;
                }
            }
        }
        let result = dispatch(state, &child, intent, &witness).await;
        let deadline = settle_result(&child, result);
        let stop = child.attempt().is_none_or(|a| {
            matches!(a.phase, Phase::Unknown | Phase::NotSent) || a.retry_at > now()
        });
        if stop {
            family::stop(&work);
        }
        if !publish(state, &child, deadline).await {
            family::stop(&work);
            return;
        }
        if stop {
            return;
        }
    }
}
async fn dispatch(
    state: Signal<ShellState>,
    work: &Work,
    intent: ReactionIntent,
    witness: &Witness,
) -> Result<Value, ListCommentReactionFailure> {
    let (client, session) =
        inputs(&state.peek(), work).map_err(ListCommentReactionFailure::NotRequested)?;
    let factory = if work.adding {
        api::reactions_add
    } else {
        api::reactions_remove
    };
    let request = factory(
        &client,
        &session,
        work.target.channel_id().into(),
        work.ts.clone(),
        work.name.clone(),
    );
    if !work.current(&state.peek())
        || !witness.current(&state.peek())
        || cooldown(&state.peek()) > now()
    {
        return Err(ListCommentReactionFailure::NotRequested(unavailable()));
    }
    let result = if crate::fixture::is_fixture() {
        fixture::request(state, work.id, &request).await
    } else {
        let context = work
            .owner
            .scope
            .request(&state.peek())
            .ok_or_else(|| ListCommentReactionFailure::NotRequested(unavailable()))?;
        context
            .transport
            .execute_list_comment_reaction(
                request,
                work.target.channel_id(),
                &work.ts,
                &work.name,
                work.adding,
                || {
                    work.current(&state.peek())
                        && witness.current(&state.peek())
                        && cooldown(&state.peek()) <= now()
                },
            )
            .await
    };
    match result {
        Ok(raw) if intent.record().classify(&raw) == ReactionResponse::Confirmed => {
            intent
                .accept(raw.clone())
                .map_err(|_| ListCommentReactionFailure::Unconfirmed(unavailable()))?;
            Ok(raw)
        }
        other => other,
    }
}
fn settle_result(work: &Work, result: Result<Value, ListCommentReactionFailure>) -> i64 {
    let deadline = match &result {
        Err(
            ListCommentReactionFailure::NotRequested(e)
            | ListCommentReactionFailure::Unconfirmed(e),
        ) => failure_deadline(e),
        Ok(raw) if raw["error"] == "ratelimited" => now().saturating_add(30),
        _ => 0,
    };
    // The ledger is shared across account replacement and credential-free.
    // Retain issued evidence without triggering a render before native custody.
    settle(work, result);
    work.update(|a| a.retry_at = a.retry_at.max(deadline));
    deadline
}
async fn publish(mut state: Signal<ShellState>, work: &Work, deadline: i64) -> bool {
    if !work.owner.active(&state.peek()) {
        return false;
    }
    let Ok(lease) = draft::seal_for_render(state).await else {
        retire_preflight(&mut state.write(), work);
        return false;
    };
    {
        let mut shell = state.write();
        if work.owner.active(&shell) {
            if deadline > now() {
                let deadline = deadline.max(cooldown(&shell));
                shell.slack_lists.comments.cooldown =
                    Some((work.owner.scope.receipt_only(), deadline));
                if let Some(e) = shell.slack_lists.comments.entry.as_mut() {
                    e.retry_at = e.retry_at.max(deadline);
                }
            }
            finish(&mut shell, &work);
        }
    }
    if let Some(lease) = lease {
        if lease.release().await.is_err() {
            return false;
        }
    }
    controller::arm_retry(state, work.owner.serial);
    true
}
