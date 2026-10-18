//! Each explicit action reserves synchronously. Network replies always return to
//! their captured storage owner, even when the visible account has changed.
use super::*;
use super_platinum_core::slack::Error;

pub(super) fn create(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: Owner,
    intent: model::Intent,
) -> Result<String, String> {
    if intent.owner() != &owner.storage.owner {
        return Err("The scheduling owner changed.".into());
    }
    let id = intent.client_msg_id().to_owned();
    if book.peek().entries.contains_key(&id) {
        return Err("This scheduled forward is already retained.".into());
    }
    book.write().entries.insert(
        id.clone(),
        Entry {
            storage: owner.storage.clone(),
            intent: intent.clone(),
            record: None,
            completion: None,
            unsent: None,
            local: None,
            run: None,
            busy: true,
            capture: None,
            pending_edit: None,
            discovery: None,
            error: String::new(),
        },
    );
    let key = id.clone();
    dioxus::core::spawn_forever(async move {
        let saved = owner
            .storage
            .run(move |journal| journal.create(intent, now()))
            .await;
        match saved {
            Ok(record) => {
                {
                    let mut root = book.write();
                    let Some(entry) = root.entries.get_mut(&key) else {
                        return;
                    };
                    entry.record = Some(record.clone());
                }
                if !owner.current(&state.peek()) {
                    fail(book,&key,"The account changed. Your scheduled forward is saved locally; no request was made.".into());
                    return;
                }
                dispatch(state, book, owner, key, record, false, None).await;
            }
            Err(error) => fail(book, &key, error),
        }
    });
    Ok(id)
}

/// Retry only the failed local save. A second click is required to schedule a
/// newly persisted Ready record; a Completion can never issue another request.
pub(super) fn retry_storage(state: Signal<ShellState>, mut book: Signal<Registry>, id: String) {
    let Ok(owner) = Owner::capture(state) else {
        return;
    };
    let (intent, completion, unsent, local) = {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(&id) else {
            return;
        };
        if entry.storage != owner.storage || entry.busy {
            return;
        }
        entry.busy = true;
        entry.error.clear();
        (
            entry.intent.clone(),
            entry.completion.clone(),
            entry.unsent.clone(),
            entry.local.clone(),
        )
    };
    dioxus::core::spawn_forever(async move {
        let resolved_conflict = matches!(&local, Some(storage::Local::Resolve { .. }));
        let saved = if let Some(unsent) = unsent {
            owner.storage.unsent(unsent).await
        } else if let Some(local) = local {
            owner.storage.local(local).await
        } else if let Some(completion) = completion {
            owner.storage.finish(completion).await
        } else {
            owner
                .storage
                .run(move |journal| journal.create(intent, now()))
                .await
        };
        if resolved_conflict && saved.is_ok() {
            release_resolved(book, &id);
        }
        publish(state, book, &owner, &id, saved);
    });
}

pub(super) fn mutate(
    state: Signal<ShellState>,
    book: Signal<Registry>,
    expected: model::Record,
    delete: bool,
    edit: Option<(Vec<serde_json::Value>, i64)>,
) {
    let Ok(owner) = Owner::capture(state) else {
        return;
    };
    mutate_owned(state, book, owner, expected, delete, edit);
}
pub(super) fn begin_capture(
    mut book: Signal<Registry>,
    owner: &Owner,
    expected: &model::Record,
) -> Result<String, String> {
    let mut root = book.write();
    let entry = root
        .entries
        .get_mut(expected.id())
        .ok_or("The retained editor is unavailable.")?;
    if entry.storage != owner.storage
        || entry.busy
        || entry.record.as_ref() != Some(expected)
        || entry.completion.is_some()
        || entry.unsent.is_some()
        || entry.local.is_some()
    {
        return Err("This retained forward changed or is already busy.".into());
    }
    let token = api::ScheduleMessage::new(String::new(), None, vec![], 0).client_msg_id;
    entry.busy = true;
    entry.capture = Some(token.clone());
    entry.error.clear();
    Ok(token)
}
pub(super) fn cancel_capture(mut book: Signal<Registry>, id: &str, token: &str, message: String) {
    if let Some(entry) = book.write().entries.get_mut(id) {
        if entry.capture.as_deref() == Some(token) {
            entry.capture = None;
            entry.busy = false;
            entry.error = message;
        }
    }
}
pub(super) fn captured_edit(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: Owner,
    expected: model::Record,
    token: String,
    note: Vec<serde_json::Value>,
    time: i64,
) {
    {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(expected.id()) else {
            return;
        };
        if entry.capture.as_ref() != Some(&token)
            || entry.storage != owner.storage
            || entry.record.as_ref() != Some(&expected)
        {
            return;
        }
        entry.capture = None;
        entry.busy = false;
    }
    mutate_owned(state, book, owner, expected, false, Some((note, time)));
}
fn mutate_owned(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: Owner,
    expected: model::Record,
    delete: bool,
    edit: Option<(Vec<serde_json::Value>, i64)>,
) {
    let id = expected.id().to_owned();
    let record = {
        let root = book.peek();
        let Some(entry) = root.entries.get(&id) else {
            return;
        };
        if entry.storage != owner.storage
            || entry.busy
            || (entry.completion.is_some() || entry.unsent.is_some() || entry.local.is_some())
        {
            return;
        }
        let Some(record) = entry.record.clone() else {
            return;
        };
        if record != expected
            || now() < record.retry_at()
            || matches!(
                record.phase(),
                model::Phase::Unknown | model::Phase::Conflict | model::Phase::Deleted
            )
        {
            return;
        }
        record
    };
    // The actual server draft ID shares the preexisting editor/attachment fence.
    let run = match record.server_id().filter(|_| owner.current(&state.peek())) {
        Some(server) => {
            match crate::scheduled::mutation::Run::detached(
                state,
                server,
                if delete {
                    crate::scheduled::attachments::other::Action::Delete
                } else {
                    crate::scheduled::attachments::other::Action::Edit
                },
            ) {
                Ok(run) => Some(run),
                Err(_) => {
                    fail(book,&id,"This draft is being edited or reviewed elsewhere. Close that editor first.".into());
                    return;
                }
            }
        }
        None => None,
    };
    {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(&id) else {
            return;
        };
        entry.busy = true;
        entry.error.clear();
        entry.run = run;
        entry.pending_edit = edit.clone();
    }
    dioxus::core::spawn_forever(dispatch(state, book, owner, id, record, delete, edit));
}

async fn dispatch(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: Owner,
    id: String,
    mut record: model::Record,
    delete: bool,
    edit: Option<(Vec<serde_json::Value>, i64)>,
) {
    let result: Result<model::Attempt, String> = async {
        if let Some((note,time))=edit {
            let expected=record.clone();
            record=retain_local(book,&owner,&id,storage::Local::Edit{expected,note,time,at:now()}).await?;
            {let mut root=book.write();if let Some(entry)=root.entries.get_mut(&id) {entry.intent=record.intent().clone();entry.record=Some(record.clone());entry.pending_edit=None;}}
        }
        if !owner.current(&state.peek()){return Err("The account changed; your updated input is saved locally.".into());}
        if !delete && record.action()==model::Action::Schedule && !owner.eligible(&state.peek(),record.intent()) {
            return Err("The source privacy or recipient availability changed. Your complete note and time remain saved; review the destination before scheduling.".into());
        }
        let preflight=if let Some(server)=record.server_id() {
            let request=api::drafts_info(&owner.context.client,&owner.context.session,server).map_err(|_|"The scheduled draft could not be requested.")?;
            let response=match fixture::execute(&owner.context,request,false).await {
                Ok(raw)=>raw,
                Err(Error::RateLimited {retry_after_secs})=> {
                    let expected=record.clone();
                    let delayed=retain_local(book,&owner,&id,storage::Local::Cooldown{expected,seconds:retry_after_secs.unwrap_or(30),at:now()}).await?;
                    {let mut root=book.write();if let Some(entry)=root.entries.get_mut(&id){entry.record=Some(delayed);}}
                    return Err("Slack asked us to wait before reading this draft again. Your input is saved.".into());
                }
                Err(error)=>return Err(crate::scheduled::failure(&error)),
            };
            let expected=record.clone();let raw=response.clone();
            record=retain_local(book,&owner,&id,storage::Local::Preflight{expected,raw,at:now()}).await?;
            {let mut root=book.write();if let Some(entry)=root.entries.get_mut(&id){entry.record=Some(record.clone());}}
            if record.phase()==model::Phase::Conflict {return Err(record.outcome().to_owned());}
            Some(response)
        }else{None};
        if !owner.current(&state.peek()) {
            return Err("The account changed; your complete input is retained.".into());
        }
        let expected = record.clone();
        owner
            .storage
            .run(move |j| j.reserve(&expected, delete, preflight.as_ref(), now()))
            .await
    }
    .await;
    let mut attempt = match result {
        Ok(attempt) => attempt,
        Err(error) => {
            release_unsent(book, &id);
            fail(book, &id, error);
            return;
        }
    };
    {
        let mut root = book.write();
        if let Some(entry) = root.entries.get_mut(&id) {
            entry.record = Some(attempt.record().clone());
        }
    }
    if !owner.current(&state.peek())
        || (attempt.record().action() == model::Action::Schedule
            && !owner.eligible(&state.peek(), attempt.record().intent()))
    {
        return_unsent(state, book, &owner, &id, attempt).await;
        return;
    }
    let request = match attempt.take_request(&owner.context.client, &owner.context.session) {
        Ok(request) => request,
        Err(_) => {
            return_unsent(state, book, &owner, &id, attempt).await;
            return;
        }
    };
    let mut run = {
        book.write()
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.run.take())
    };
    let response = if let Some(run) = run.as_mut() {
        run.dispatch_retained(state, request).await
    } else {
        fixture::execute(&owner.context, request, true).await
    };
    // The root holds the exact response before starting any fallible local write.
    let completion = Arc::new(Mutex::new(attempt.complete(response, now())));
    {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(&id) else {
            return;
        };
        entry.completion = Some(completion.clone());
        entry.run = run;
    }
    let saved = owner.storage.finish(completion).await;
    publish(state, book, &owner, &id, saved);
}

async fn return_unsent(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: &Owner,
    id: &str,
    attempt: model::Attempt,
) {
    let proof = Arc::new(Mutex::new(attempt.unsent(now())));
    {
        let mut root = book.write();
        if let Some(entry) = root.entries.get_mut(id) {
            entry.unsent = Some(proof.clone());
        }
    }
    release_unsent(book, id);
    let saved = owner.storage.unsent(proof).await;
    publish(state, book, owner, id, saved);
}

fn release_unsent(mut book: Signal<Registry>, id: &str) {
    let run = {
        book.write()
            .entries
            .get_mut(id)
            .and_then(|entry| entry.run.take())
    };
    drop(run);
}
fn publish(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    owner: &Owner,
    id: &str,
    saved: Result<model::Record, String>,
) {
    match saved {
        Ok(record) => {
            let mut finished = None;
            {
                let mut root = book.write();
                let Some(entry) = root.entries.get_mut(id) else {
                    return;
                };
                if entry.pending_edit.as_ref().is_some_and(|(note, time)| {
                    note.as_slice() == record.intent().note() && *time == record.intent().post_at()
                }) {
                    entry.pending_edit = None;
                }
                entry.discovery = None;
                entry.intent = record.intent().clone();
                entry.record = Some(record.clone());
                entry.completion = None;
                entry.unsent = None;
                entry.local = None;
                entry.busy = false;
                entry.error.clear();
                if !matches!(
                    record.phase(),
                    model::Phase::Unknown | model::Phase::Conflict
                ) {
                    finished = entry.run.take();
                }
            }
            // Storage and ownership settlement are independent of current UI authority.
            if let Some(mut run) = finished {
                run.settle_retained(matches!(
                    record.phase(),
                    model::Phase::Confirmed | model::Phase::Deleted
                ));
            }
            accept(state, owner, &record);
        }
        Err(error) => fail(book, id, error),
    }
}

/// One explicit page or one known-ID read. No automatic page loop or resend.
pub(super) fn review(state: Signal<ShellState>, mut book: Signal<Registry>, id: String) {
    let Ok(owner) = Owner::capture(state) else {
        return;
    };
    let (record, scan, known_server) = {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(&id) else {
            return;
        };
        let Some(record) = entry.record.clone() else {
            return;
        };
        if entry.storage != owner.storage
            || entry.busy
            || entry.completion.is_some()
            || entry.unsent.is_some()
            || entry.local.is_some()
            || now() < record.retry_at()
            || !matches!(
                record.phase(),
                model::Phase::Unknown | model::Phase::Conflict
            )
        {
            return;
        }
        let known_server = record
            .server_id()
            .or_else(|| {
                record
                    .observed()
                    .and_then(|v| v.get("draft"))
                    .and_then(|v| v.get("id"))
                    .and_then(serde_json::Value::as_str)
            })
            .map(str::to_owned);
        let scan = if known_server.is_none() {
            match entry
                .discovery
                .as_ref()
                .filter(|scan| scan.matches(&record))
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| model::discovery::Scan::new(&record))
            {
                Ok(scan) => Some(scan),
                Err(error) => {
                    entry.error = error.into();
                    return;
                }
            }
        } else {
            None
        };
        entry.busy = true;
        entry.error.clear();
        (record, scan, known_server)
    };
    dioxus::core::spawn_forever(async move {
        let response = if let Some(scan) = &scan {
            match scan.request(&owner.context.client, &owner.context.session) {
                Ok(request) if owner.current(&state.peek()) => {
                    fixture::execute(&owner.context, request, false).await
                }
                Ok(_) => Err(Error::Api("scheduled_forward_owner_changed".into())),
                Err(error) => Err(error),
            }
        } else {
            read_discovered(state, &owner, known_server.as_deref().unwrap()).await
        };
        let response = match (response, scan) {
            (Ok(raw), Some(scan)) => match scan.accept(&raw) {
                Ok(model::discovery::Page::Found(server)) => {
                    // The page is discovery, not canonical readback. Revalidate
                    // current owner before an additional exact-ID read.
                    if !owner.current(&state.peek()) {
                        fail(book,&id,"The account changed. No further read was made; your forward remains retained.".into());
                        return;
                    }
                    read_discovered(state, &owner, &server).await
                }
                Ok(model::discovery::Page::More(next)) => {
                    let mut root = book.write();
                    if let Some(entry) = root.entries.get_mut(&id) {
                        if entry.storage == owner.storage && entry.record.as_ref() == Some(&record)
                        {
                            entry.discovery = Some(next);
                            entry.busy = false;
                            entry.error="No matching draft was in this page. Continue checking older drafts; nothing will be resent.".into();
                        }
                    }
                    return;
                }
                Ok(model::discovery::Page::Exhausted) => {
                    if let Some(entry) = book.write().entries.get_mut(&id) {
                        entry.discovery = None;
                    }
                    fail(book,&id,"No complete match was found. Absence does not prove failure; your forward remains retained and will not be resent. You can check again.".into());
                    return;
                }
                Err(error) => {
                    if let Some(entry) = book.write().entries.get_mut(&id) {
                        entry.discovery = None;
                    }
                    fail(book, &id, error.into());
                    return;
                }
            },
            (response, _) => response,
        };
        let saved = match response {
            Ok(raw) => {
                retain_local(
                    book,
                    &owner,
                    &id,
                    storage::Local::Review {
                        expected: record,
                        raw,
                        at: now(),
                    },
                )
                .await
            }
            Err(Error::RateLimited { retry_after_secs }) => {
                retain_local(
                    book,
                    &owner,
                    &id,
                    storage::Local::Cooldown {
                        expected: record,
                        seconds: retry_after_secs.unwrap_or(30),
                        at: now(),
                    },
                )
                .await
            }
            Err(error) => Err(crate::scheduled::failure(&error)),
        };
        publish(state, book, &owner, &id, saved);
    });
}

async fn read_discovered(
    state: Signal<ShellState>,
    owner: &Owner,
    server: &str,
) -> Result<serde_json::Value, Error> {
    if !owner.current(&state.peek()) {
        return Err(Error::Api("scheduled_forward_owner_changed".into()));
    }
    let request = api::drafts_info(&owner.context.client, &owner.context.session, server)?;
    let raw = fixture::execute(&owner.context, request, false).await?;
    // No alternate row with the same client UUID may replace the exact ID found.
    if raw
        .get("draft")
        .and_then(|v| v.get("id"))
        .and_then(serde_json::Value::as_str)
        != Some(server)
    {
        return Err(Error::Api(
            "scheduled_forward_discovery_identity_changed".into(),
        ));
    }
    Ok(raw)
}

pub(super) async fn retain_local(
    mut book: Signal<Registry>,
    owner: &Owner,
    id: &str,
    receipt: storage::Local,
) -> Result<model::Record, String> {
    if !receipt.admissible() {
        return Err(if matches!(&receipt,storage::Local::Edit{..}) {
            "Your captured note or time could not be saved. Your input is retained; review the note and choose a valid future time."
        }else{
            "The draft read was incomplete or changed. Your input is retained; check again before making a decision."
        }.into());
    }
    {
        let mut root = book.write();
        let entry = root
            .entries
            .get_mut(id)
            .ok_or("Scheduled forward custody is unavailable.")?;
        if entry.storage != owner.storage {
            return Err("Scheduled forward owner changed.".into());
        }
        entry.local = Some(receipt.clone());
    }
    let saved = owner.storage.local(receipt).await;
    if let Ok(record) = &saved {
        let mut root = book.write();
        if let Some(entry) = root.entries.get_mut(id) {
            entry.record = Some(record.clone());
            entry.local = None;
        }
    }
    saved
}

// An explicit local version choice only; no write is chained after the read.
pub(super) fn resolve_conflict(
    state: Signal<ShellState>,
    mut book: Signal<Registry>,
    expected: model::Record,
) {
    let Ok(owner) = Owner::capture(state) else {
        return;
    };
    let Some(observed) = expected.observed().cloned() else {
        return;
    };
    let Some(server) = observed["draft"]["id"].as_str().map(str::to_owned) else {
        return;
    };
    let id = expected.id().to_owned();
    {
        let mut root = book.write();
        let Some(entry) = root.entries.get_mut(&id) else {
            return;
        };
        if entry.storage != owner.storage
            || entry.record.as_ref() != Some(&expected)
            || entry.busy
            || entry.local.is_some()
            || entry.completion.is_some()
            || entry.unsent.is_some()
            || expected.phase() != model::Phase::Conflict
            || now() < expected.retry_at()
        {
            return;
        }
        entry.busy = true;
        entry.error.clear();
    }
    dioxus::core::spawn_forever(async move {
        let result=async {
            let request=api::drafts_info(&owner.context.client,&owner.context.session,&server).map_err(|e|crate::scheduled::failure(&e))?;
            let raw=match fixture::execute(&owner.context,request,false).await {
                Ok(raw)=>raw,
                Err(Error::RateLimited{retry_after_secs})=> {
                    retain_local(book,&owner,&id,storage::Local::Cooldown{expected:expected.clone(),seconds:retry_after_secs.unwrap_or(30),at:now()}).await?;
                    return Err("Slack asked us to wait before reviewing this draft again.".into());
                }
                Err(error)=>return Err(crate::scheduled::failure(&error)),
            };
            if !owner.current(&state.peek()) {return Err("The account changed; both saved versions remain unchanged.".into());}
            if raw.get("draft")!=observed.get("draft") {return Err("The Slack version changed again. Check the scheduled forward and review the new version before choosing.".into());}
            retain_local(book,&owner,&id,storage::Local::Resolve{expected,raw,at:now()}).await
        }.await;
        if result.is_ok() {
            release_resolved(book, &id);
        }
        publish(state, book, &owner, &id, result);
    });
}
fn release_resolved(mut book: Signal<Registry>, id: &str) {
    let run = {
        book.write()
            .entries
            .get_mut(id)
            .and_then(|entry| entry.run.take())
    };
    if let Some(mut run) = run {
        run.resolved_retained_conflict();
    }
}
