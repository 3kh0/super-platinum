use super::*;

pub(crate) fn save(state: Signal<ShellState>, binding: Binding) {
    if !presentation(&state.peek()).is_some_and(|p| p.binding.token == binding.token && p.can_save)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        save_owned(state, binding).await;
    });
}
async fn save_owned(mut state: Signal<ShellState>, binding: Binding) {
    let lease = match bridge::capture(state, &binding).await {
        Ok(v) => v,
        Err(_) => {
            notice(
                state,
                &binding,
                "Finish composing or close the link editor before saving. Both drafts are retained.",
            );
            return;
        }
    };
    let start = {
        let mut shell = state.write();
        if !presentation(&shell).is_some_and(|p| p.binding.token == binding.token && p.can_save)
            || !suspension_current(&shell, &binding)
        {
            return;
        }
        let Some((_, serial, _, _)) = controller::begin_preflight(&mut shell) else {
            return;
        };
        let s = session_mut(&mut shell, &binding).unwrap();
        s.busy = true;
        s.notice = Some("Checking permission to save this comment…".into());
        (serial, s.seed.clone(), s.revision)
    };
    let (serial, seed, revision) = start;
    let prepared = prepare(state, &binding.source, serial)
        .await
        .and_then(|(loaded, policy)| {
            let shell = state.peek();
            if !binding.current(&shell)
                || !suspension_current(&shell, &binding)
                || !session(&shell, &binding)
                    .is_some_and(|s| s.revision == revision && s.blocks == lease.blocks())
            {
                return Err(unavailable());
            }
            let known = selected(&loaded.page, &binding.source.ts)?;
            seed.revalidate(&loaded.page, known, policy.policy(), millis())
                .map_err(|e| Error::Api(e.to_string()))?;
            let intent = OwnEditIntent::capture(
                &seed,
                known,
                policy.policy(),
                lease.blocks().to_vec(),
                millis(),
            )
            .map_err(|e| Error::Api(e.to_string()))?;
            let (client, workspace) = inputs(&shell, &binding.source)?;
            let request = api::list_record_comment_update(&client, &workspace, &intent)?;
            Ok((intent, request, policy))
        });
    let (intent, request, policy) = match prepared {
        Ok(v) => v,
        Err(error) => {
            publish::finish_read(
                &mut state.write(),
                &binding.source.owner,
                serial,
                Some(&error),
            );
            notice(
                state,
                &binding,
                "The edit was not sent. Permissions or the original comment changed. Both drafts are retained.",
            );
            let _ = lease.release().await;
            controller::arm_retry(state, binding.source.owner.serial);
            return;
        }
    };
    if lease.validate().await.is_err() {
        publish::finish_read(&mut state.write(), &binding.source.owner, serial, None);
        notice(
            state,
            &binding,
            "The edit was not sent because native capture changed. Both drafts are retained.",
        );
        return;
    }
    let attempt = {
        let mut shell = state.write();
        if !binding.current(&shell)
            || !policy.current(&shell)
            || !binding.source.owner.reading(&shell, serial)
            || !suspension_current(&shell, &binding)
        {
            publish::finish_read(&mut shell, &binding.source.owner, serial, None);
            if let Some(s) = session_mut(&mut shell, &binding) {
                s.busy = false;
                s.notice = Some(
                    "The edit was not sent. The original comment or permission changed.".into(),
                );
            }
            return;
        }
        let account = shell.core.active_account.clone();
        let all = &mut shell.slack_lists.comments.own_edit.attempts;
        while all.len() >= 32 {
            let Some(index) = all.iter().position(|a| !a.lock().unwrap().phase.blocks()) else {
                break;
            };
            all.remove(index);
        }
        if all.len() >= 32 {
            publish::finish_read(&mut shell, &binding.source.owner, serial, None);
            if let Some(s) = session_mut(&mut shell, &binding) {
                s.busy = false;
                s.notice =
                    Some("Resolve earlier unconfirmed comment edits before saving another.".into());
            }
            return;
        }
        let a = Arc::new(Mutex::new(Attempt {
            id: next_id(),
            disk_id: None,
            account,
            record: intent.record().clone(),
            phase: Phase::Preparing,
            receipt: None,
            retry_at: 0,
        }));
        all.push(a.clone());
        let s = session_mut(&mut shell, &binding).unwrap();
        s.attempt = Some(a.clone());
        s.notice = Some("Saving the edited comment…".into());
        publish::finish_read(&mut shell, &binding.source.owner, serial, None);
        a
    };
    let durable = session(&state.peek(), &binding).and_then(|s| s.durable.clone());
    let Some(durable) = durable else {
        attempt.lock().unwrap().phase = Phase::NotSent;
        notice(
            state,
            &binding,
            "The edit was not sent. Local recovery is unavailable; both drafts are retained.",
        );
        return;
    };
    let disk_id = match durable.begin(intent.record().clone()).await {
        Ok(id) => id,
        Err(_) => {
            attempt.lock().unwrap().phase = Phase::NotSent;
            notice(
                state,
                &binding,
                "The edit was not sent because its recovery record could not be saved. Try Save again after local storage recovers; both drafts are retained.",
            );
            return;
        }
    };
    {
        let mut a = attempt.lock().unwrap();
        a.disk_id = Some(disk_id.clone());
        a.phase = Phase::Sent;
    }
    // The durable lock is already committed. Authority can retire during that
    // I/O, so recheck before entering either fixture or real transport.
    let current = {
        let shell = state.peek();
        binding.current(&shell)
            && policy.current(&shell)
            && suspension_current(&shell, &binding)
            && session(&shell, &binding).is_some_and(|s| s.revision == revision)
    };
    // No cancellation race after issuance: an original late result belongs in
    // this attempt even when a different account or view is now visible.
    let result = if !current || lease.validate().await.is_err() {
        Err(ListCommentUpdateFailure::NotRequested(unavailable()))
    } else if crate::fixture::is_fixture() {
        fixture::own_edit::update(state, &request, intent).await
    } else {
        let context = binding.source.owner.scope.request(&state.peek());
        match context {
            Some(ctx) => {
                ctx.transport
                    .execute_list_comment_update(request, intent, || {
                        binding.current(&state.peek())
                            && policy.current(&state.peek())
                            && session(&state.peek(), &binding)
                                .is_some_and(|s| s.revision == revision)
                    })
                    .await
            }
            None => Err(ListCommentUpdateFailure::NotRequested(unavailable())),
        }
    };
    settle(&attempt, &result);
    let (phase, deadline, raw) = {
        let a = attempt.lock().unwrap();
        (a.phase, a.retry_at, a.receipt.clone())
    };
    let outcome = match &result {
        Ok(accepted) => durable::Outcome::Confirmed(accepted.clone()),
        Err(ListCommentUpdateFailure::NotRequested(_)) => durable::Outcome::NotSent,
        Err(ListCommentUpdateFailure::Rejected(raw)) => durable::Outcome::Rejected(raw.clone()),
        _ if phase == Phase::Rejected && raw.is_none() => durable::Outcome::RateLimited,
        _ => {
            attempt.lock().unwrap().phase = Phase::Unknown;
            durable::Outcome::Uncertain(raw)
        }
    };
    if durable.resolve(disk_id, outcome, deadline).await.is_err() {
        notice(
            state,
            &binding,
            "The save result is retained here but could not be saved locally. Retry Cancel or Check result after storage recovers; this attempt will not be sent again.",
        );
        let _ = lease.release().await;
        return;
    }
    {
        let mut shell = state.write();
        let deadline = attempt.lock().unwrap().retry_at.max(cooldown(&shell));
        if binding.source.owner_current(&shell) && deadline > now() {
            shell.slack_lists.comments.cooldown =
                Some((binding.source.owner.scope.clone(), deadline));
            shell.slack_lists.comments.entry.as_mut().unwrap().retry_at = deadline;
        }
    }
    controller::arm_retry(state, binding.source.owner.serial);
    if !binding.visible_mount(&state.peek()) {
        return;
    }
    if lease.validate().await.is_err() {
        notice(
            state,
            &binding,
            "The save result is retained. Native edit recovery is required before returning to the reply.",
        );
        return;
    }
    let success = {
        let mut shell = state.write();
        let phase = attempt.lock().unwrap().phase;
        let same = session(&shell, &binding).is_some_and(|s| s.revision == revision);
        let applied = match &result {
            Ok(value) if same => publish::accepted(&mut shell, &binding, value),
            _ => false,
        };
        if let Some(s) = session_mut(&mut shell, &binding) {
            s.busy = false;
            s.notice=Some(match phase{
            Phase::Confirmed if applied=>"The comment was saved.",
            Phase::Confirmed=>"Slack accepted the edit. A newer comment is retained; refresh after returning to the reply.",
            Phase::Rejected=>"Slack did not accept the edit. Your edit and reply draft are retained.",
            Phase::NotSent=>"The edit was not sent. Your edit and reply draft are retained.",
            _=>"The edit may have reached Slack. Check the result before saving again; both drafts are retained.",
        }.into());
        }
        applied
    };
    if success {
        restore(state, &binding, lease).await;
    } else {
        let _ = lease.release().await;
    }
}
pub(in crate::slack_lists::comments::own_edit) fn settle(
    attempt: &Arc<Mutex<Attempt>>,
    result: &Result<OwnEditAccepted, ListCommentUpdateFailure>,
) {
    let mut a = attempt.lock().unwrap();
    match result {
        Ok(v) => {
            a.phase = Phase::Confirmed;
            a.receipt = Some(v.raw().clone());
        }
        Err(ListCommentUpdateFailure::NotRequested(error)) => {
            a.phase = Phase::NotSent;
            a.retry_at = retry_deadline(error);
        }
        Err(ListCommentUpdateFailure::Rejected(raw)) => {
            a.phase = Phase::Rejected;
            a.receipt = Some(raw.clone());
            if raw["error"] == "ratelimited" {
                a.retry_at = retry_deadline(&Error::RateLimited {
                    retry_after_secs: raw["retry_after"].as_u64(),
                });
            }
        }
        Err(ListCommentUpdateFailure::Unconfirmed { error, receipt }) => {
            a.phase = if matches!(
                error,
                Error::RateLimited { .. } | Error::HttpStatus { status: 429, .. }
            ) {
                Phase::Rejected
            } else {
                Phase::Unknown
            };
            a.retry_at = retry_deadline(error);
            a.receipt = receipt.clone();
        }
    }
}
pub(crate) fn check_result(state: Signal<ShellState>, binding: Binding) {
    if !presentation(&state.peek()).is_some_and(|p| p.binding.token == binding.token && p.can_check)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        check_owned(state, binding).await;
    });
}
async fn check_owned(mut state: Signal<ShellState>, binding: Binding) {
    let lease = match bridge::capture(state, &binding).await {
        Ok(v) => v,
        Err(_) => {
            notice(
                state,
                &binding,
                "Finish composing or close the link editor before checking the result.",
            );
            return;
        }
    };
    let (serial, attempt, events) = {
        let mut shell = state.write();
        if !presentation(&shell).is_some_and(|p| p.binding.token == binding.token && p.can_check)
            || !suspension_current(&shell, &binding)
        {
            return;
        }
        let Some((_, serial, _, _)) = controller::begin_preflight(&mut shell) else {
            return;
        };
        let events = shell.slack_lists.comments.own_edit.events;
        let s = session_mut(&mut shell, &binding).unwrap();
        s.busy = true;
        s.notice = Some("Checking the current comment…".into());
        (serial, s.attempt.clone().unwrap(), events)
    };
    let result = loaded(state, &binding.source, serial).await;
    if lease.validate().await.is_err() {
        publish::finish_read(&mut state.write(), &binding.source.owner, serial, None);
        notice(
            state,
            &binding,
            "Native capture changed. The edit remains unconfirmed.",
        );
        return;
    }
    let positive_page = {
        let shell = state.peek();
        if binding.visible_mount(&shell)
            && binding.source.owner_current(&shell)
            && binding.source.owner.reading(&shell, serial)
            && shell.slack_lists.comments.own_edit.events == events
        {
            result
                .as_ref()
                .ok()
                .filter(|loaded| {
                    attempt
                        .lock()
                        .unwrap()
                        .record
                        .observed(&loaded.page)
                        .is_ok_and(|m| m.is_some())
                })
                .map(|loaded| loaded.page.clone())
        } else {
            None
        }
    };
    if let Some(page) = positive_page {
        let durable = session(&state.peek(), &binding).and_then(|s| s.durable.clone());
        let (disk_id, retry_at) = {
            let a = attempt.lock().unwrap();
            (a.disk_id.clone(), a.retry_at)
        };
        let persisted = match (durable, disk_id) {
            (Some(durable), Some(id)) => durable
                .resolve(id, durable::Outcome::Observed(page), retry_at)
                .await
                .is_ok(),
            _ => false,
        };
        if !persisted {
            publish::finish_read(&mut state.write(), &binding.source.owner, serial, None);
            notice(
                state,
                &binding,
                "The edited content was found, but local recovery could not be saved. Both drafts are retained. Check again after storage recovers.",
            );
            let _ = lease.release().await;
            return;
        }
        attempt.lock().unwrap().phase = Phase::Observed;
    }
    let observed = {
        let mut shell = state.write();
        if !binding.visible_mount(&shell)
            || !binding.source.owner_current(&shell)
            || !binding.source.owner.reading(&shell, serial)
            || shell.slack_lists.comments.own_edit.events != events
        {
            publish::finish_read(&mut shell, &binding.source.owner, serial, None);
            if let Some(s) = session_mut(&mut shell, &binding) {
                s.busy = false;
            }
            return;
        }
        let positive = result.as_ref().is_ok_and(|loaded| {
            attempt
                .lock()
                .unwrap()
                .record
                .observed(&loaded.page)
                .is_ok_and(|m| m.is_some())
        });
        // A readback is canonical read evidence; it never becomes the original
        // update ACK or overwrites that attempt's retained raw response.
        controller::finish(&mut shell, &binding.source.owner, serial, None, result);
        if positive {
            attempt.lock().unwrap().phase = Phase::Observed;
        }
        if let Some(s) = session_mut(&mut shell, &binding) {
            s.busy = false;
            s.notice=Some(if positive{"The edited content was found in a fresh read. The original save acknowledgement is still unconfirmed."}else{"The edited content could not be confirmed. Both drafts are retained; this save will not be sent again."}.into());
        }
        positive
    };
    let same_buffer = session(&state.peek(), &binding).is_some_and(is_attempt_buffer);
    if observed && same_buffer {
        restore(state, &binding, lease).await;
    } else {
        if observed {
            notice(
                state,
                &binding,
                "The saved content was found. Your newer edit is retained; Cancel discards that edit and returns to your reply.",
            );
        }
        let _ = lease.release().await;
    }
}
