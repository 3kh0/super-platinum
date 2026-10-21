use super::*;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    let (lease, original) = {
        let s = state.peek();
        let Some(a) = s
            .app_home
            .about
            .management
            .deletions
            .attempts
            .iter()
            .find(|a| a.original.id == id)
        else {
            return;
        };
        (a.lease.clone(), a.original.clone())
    };
    let preflight = async {
        if !exact(&state.peek(), id) {
            return Err(unavailable());
        }
        if !native::current(&original).await {
            return Err(unavailable());
        }
        let fresh_policy = policy::acquire(state, &lease).await?;
        let fresh = read::load_editor(state, &lease, original.editor.workflow_id()).await?;
        if !exact(&state.peek(), id)
            || !fresh_policy.current(&state.peek())
            || !fresh_policy.same(&original.policy)
            || !fresh_policy.can_create()
            || !allowed_editor(&fresh, &lease.owner.ctx.session.user_id)
            || !original.editor.same_revision(&fresh)
        {
            return Err(unavailable());
        }
        if !native::current(&original).await {
            return Err(unavailable());
        }
        if !exact(&state.peek(), id) {
            return Err(unavailable());
        }
        Ok(())
    }
    .await;
    if preflight.is_err() {
        finish_native(
            state,
            id,
            &original,
            false,
            "Workflow or permissions changed. No deletion was sent; your draft is retained.",
        )
        .await;
        return;
    }
    let count = {
        let s = state.peek();
        s.app_home
            .about
            .management
            .deletions
            .attempts
            .iter()
            .find(|a| a.original.id == id)
            .unwrap()
            .plan
            .slots()
            .len()
    };
    for index in 0..count {
        if !exact(&state.peek(), id) {
            break;
        }
        if !native::current(&original).await {
            break;
        }
        let request = {
            let mut s = state.write();
            if !exact(&s, id) {
                break;
            }
            let a = s
                .app_home
                .about
                .management
                .deletions
                .attempts
                .iter_mut()
                .find(|a| a.original.id == id)
                .unwrap();
            if a.plan.mark_sent(index).is_err() {
                break;
            }
            a.plan.slots()[index]
                .plan()
                .request(&lease.owner.ctx.client, &lease.owner.ctx.session)
        };
        let result = if fixture::enabled(&state.peek()) {
            fixture::delete::mutation(state, &request)
                .await
                .unwrap_or_else(|| Err(Error::Api("workflow_management_not_sent_fixture".into())))
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_delete(request)
                .await
        };
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        settle(&mut s, id, index, result);
    }
    let before = exact(&state.peek(), id);
    let native_current = before && native::current(&original).await;
    let current = native_current && exact(&state.peek(), id);
    finish_native(state,id,&original,current,"Deletion is not confirmed. Each response and your draft are retained. No deletion will be replayed.").await;
}
pub(super) fn settle(s: &mut ShellState, id: u64, index: usize, result: Result<Value, Error>) {
    let Some(a) = s
        .app_home
        .about
        .management
        .deletions
        .attempts
        .iter_mut()
        .find(|a| a.original.id == id)
    else {
        return;
    };
    // Raw receipts and core validation remain separate. Duplicate or invalid
    // settlements cannot replace an earlier receipt or authorize another slot.
    if a.replies.get(index).is_none_or(Option::is_some) {
        return;
    }
    let (outcome, reply) = match result {
        Ok(raw) => {
            let outcome = if raw.is_object()
                && raw["ok"] == true
                && raw.get("error").is_none_or(Value::is_null)
            {
                Outcome::Succeeded(raw.clone())
            } else if raw.is_object() && raw["ok"] == false {
                Outcome::Rejected(raw.clone())
            } else {
                Outcome::Unknown
            };
            (outcome, Reply::Raw(raw))
        }
        Err(Error::Offline(_)) => (Outcome::NotSent, Reply::Failure("offline before dispatch")),
        Err(Error::Api(code)) if code.starts_with("workflow_management_not_sent_") => {
            (Outcome::NotSent, Reply::Failure("refused before dispatch"))
        }
        Err(_) => (Outcome::Unknown, Reply::Failure("delivery unconfirmed")),
    };
    if a.plan.settle(index, outcome).is_err() {
        if a.plan.settle(index, Outcome::Unknown).is_err() {
            return;
        }
    }
    a.replies[index] = Some(reply);
}
async fn finish_native(
    mut state: Signal<ShellState>,
    id: u64,
    original: &Original,
    current: bool,
    notice: &str,
) {
    let buffers = native::finish_snapshot(original).await;
    let mut s = state.write();
    let same_document = document(&s, original.visit).is_some_and(|d| d.id == original.document);
    let source_current = exact(&s, id);
    let same_buffers = buffers
        .as_ref()
        .is_some_and(|b| b.title == original.title && b.description == original.description);
    // A newer native buffer is retained only for this original current draft,
    // and never contributes authorization or a successful terminal decision.
    let adopt_buffers = source_current.then_some(buffers.as_ref()).flatten();
    let update_document = !same_document || adopt_buffers.is_some();
    finish_owned(
        &mut s,
        id,
        current && source_current && same_buffers,
        notice,
        update_document,
        adopt_buffers,
    );
}
#[cfg(test)]
pub(super) fn finish(s: &mut ShellState, id: u64, current: bool, notice: &str) {
    finish_owned(s, id, current, notice, true, None);
}
fn finish_owned(
    s: &mut ShellState,
    id: u64,
    current: bool,
    notice: &str,
    update_document: bool,
    buffers: Option<&native::Finish>,
) {
    let Some(a) = s
        .app_home
        .about
        .management
        .deletions
        .attempts
        .iter_mut()
        .find(|a| a.original.id == id)
    else {
        return;
    };
    if a.finished {
        return;
    }
    a.finished = true;
    a.no_send = a
        .plan
        .slots()
        .iter()
        .all(|slot| matches!(slot.outcome(), Outcome::Pending | Outcome::NotSent));
    a.terminal = current && a.plan.primary_succeeded();
    let terminal = a.terminal;
    let no_send = a.no_send;
    let doc = a.original.document;
    let lease = a.lease.clone();
    let release = a.release.take();
    if let Some(d) = s
        .app_home
        .about
        .management
        .documents
        .iter_mut()
        .find(|d| d.id == doc && d.pending == Some(id) && update_document)
    {
        if let Some(buffers) = buffers {
            d.title = buffers.title.clone();
            d.description = buffers.description.clone();
        }
        d.notice = Some(
            if terminal {
                "Workflow deleted. Your local draft and deletion responses are retained."
            } else {
                notice
            }
            .into(),
        );
        d.revision = next();
        if no_send {
            d.pending = None;
        }
    }
    lease.finish(s, None);
    if let Some(release) = release {
        release.release();
    }
}

#[cfg(test)]
mod finish_tests {
    use super::*;
    #[test]
    fn refused_finish_keeps_native_model_boundary_unchanged_but_releases_busy_read() {
        let (mut s, visit, id) = super::super::tests::prepared();
        let before = document(&s, visit).unwrap().clone();
        finish_owned(&mut s, id, false, "Refused", false, None);
        let after = document(&s, visit).unwrap();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.title, before.title);
        assert_eq!(after.description, before.description);
        assert_eq!(after.notice, before.notice);
        assert_eq!(after.pending, Some(id));
        assert!(
            s.app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .read
                .is_none()
        );
        assert!(s.app_home.about.management.deletions.attempts[0].finished);
    }
    #[test]
    fn finish_buffer_is_retained_without_changing_original_attempt_or_authorizing_terminal() {
        let (mut s, visit, id) = super::super::tests::prepared();
        let original = s.app_home.about.management.deletions.attempts[0]
            .original
            .title
            .clone();
        let buffers = native::Finish {
            title: format!("{original} native-ahead"),
            description: "Ahead native description".into(),
        };
        finish_owned(&mut s, id, false, "Retained", true, Some(&buffers));
        assert_eq!(document(&s, visit).unwrap().title, buffers.title);
        assert_eq!(
            s.app_home.about.management.deletions.attempts[0]
                .original
                .title,
            original
        );
        assert!(!terminal(&s, visit));
    }
}
