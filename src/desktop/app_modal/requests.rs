use super::*;

struct Ticket {
    context: Context,
    scope: ModalScope,
    view_id: String,
    revision: u64,
    generation: u64,
}
impl Ticket {
    fn capture(shell: &ShellState) -> Option<Self> {
        let session = shell.app_modal.as_ref()?;
        if !current_scope(shell, session) {
            return None;
        }
        Some(Self {
            context: Context::capture(shell)?,
            scope: session.modal.scope().clone(),
            view_id: session.modal.stack().last()?.id.clone(),
            revision: session.modal.revision(),
            generation: session.generation,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        crate::message_buttons::inputs::user::directory::context_current(shell, &self.context)
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| w.self_user_id == self.context.session.user_id)
            && self.same_view(shell)
    }
    fn same_view(&self, shell: &ShellState) -> bool {
        scope(shell).as_ref() == Some(&self.scope)
            && shell.overlay == Some(Overlay::AppModal)
            && shell.app_modal.as_ref().is_some_and(|s| {
                s.modal.scope() == &self.scope
                    && s.generation == self.generation
                    && s.modal.revision() == self.revision
                    && s.modal.stack().last().is_some_and(|v| v.id == self.view_id)
            })
    }
    /// A same-person credential/transport replacement loses response authority,
    /// not the fact that this exact view still has an outstanding request.
    /// Never apply the old response body or unlock a potentially sent mutation.
    fn settle_lost_authority(&self, shell: &mut ShellState, submission: bool) -> bool {
        if self.current(shell)
            || !self.same_view(shell)
            || shell.core.account_epoch != self.context.epoch
            || shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&self.context.session.team_id))
                .is_none_or(|w| w.user_id != self.context.session.user_id)
            || shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_none_or(|w| w.self_user_id != self.context.session.user_id)
        {
            return false;
        }
        let session = shell.app_modal.as_mut().unwrap();
        if submission {
            if !session.busy {
                return false;
            }
            session.busy = false;
            session.uncertain = true;
            session.view_error=Some("The connection changed before the app's response could be confirmed. Your entries are retained and the submission will not be repeated.".into());
        } else {
            if !session.loading {
                return false;
            }
            session.loading = false;
            session.conflict = true;
            session.view_error=Some("The connection changed while loading this app form. Close it and try opening it again.".into());
        }
        true
    }
}

pub(super) async fn load(mut state: Signal<ShellState>) {
    let Some(ticket) = Ticket::capture(&state.read()) else {
        return;
    };
    if !state.read().app_modal.as_ref().is_some_and(|s| s.loading) {
        return;
    }
    let result = match api::block_view_get(
        &ticket.context.client,
        &ticket.context.session,
        &ticket.view_id,
    ) {
        Ok(request) => ticket
            .context
            .transport
            .execute_block_interaction(request)
            .await
            .and_then(|v| api::decode_block_view_get(&v)),
        Err(error) => Err(error),
    };
    let mut shell = state.write();
    if !ticket.current(&shell) {
        ticket.settle_lost_authority(&mut shell, false);
        return;
    }
    let session = shell.app_modal.as_mut().unwrap();
    session.loading = false;
    match result {
        Ok(api::BlockViewResponse::Accepted(view)) => {
            if session
                .modal
                .hydrate_current(&ticket.scope, &ticket.view_id, ticket.revision, view)
            {
                session.sync_forms();
            } else {
                session.conflict = true;
                session.view_error=Some("The app returned a different form. Close this dialog and try opening it again.".into());
            }
        }
        Ok(api::BlockViewResponse::Rejected(error)) => {
            session.conflict = true;
            session.view_error = Some(
                error
                    .view_error
                    .or(error.message)
                    .unwrap_or_else(|| "This app form could not be loaded.".into()),
            );
        }
        Err(_) => {
            session.conflict = true;
            session.view_error = Some(
                "This app form could not be loaded. Close it and try opening it again.".into(),
            );
        }
    }
}

/// Recheck current modal scope and context before building any views.submit body.
/// A preselected or locally edited value cannot bypass an unavailable directory policy.
pub(super) fn prepare_submission(shell: &ShellState) -> Result<Value, FormError> {
    let session = shell
        .app_modal
        .as_ref()
        .ok_or(FormError::ConflictingUpdate)?;
    if !current_scope(shell, session)
        || shell.overlay != Some(Overlay::AppModal)
        || session.locked()
    {
        return Err(FormError::ConflictingUpdate);
    }
    if let Some(reason) = users::unavailable_context(shell) {
        return Err(FormError::Unsupported(vec![reason.into()]));
    }
    let payload = session
        .current_form()
        .ok_or(FormError::ConflictingUpdate)?
        .submission()?;
    super::channels::validate_submission(shell)?;
    Ok(payload)
}

pub(super) async fn submit(mut state: Signal<ShellState>) {
    let (ticket, payload, client_token, fixture_revision) = {
        let mut shell = state.write();
        let valid = shell
            .app_modal
            .as_ref()
            .is_some_and(|s| current_scope(&shell, s) && !s.locked())
            && shell.overlay == Some(Overlay::AppModal);
        if !valid {
            return;
        }
        let prepared = prepare_submission(&shell);
        let session = shell.app_modal.as_mut().unwrap();
        let Some(form) = session.current_form_mut() else {
            return;
        };
        if form.submit_label.is_none() {
            return;
        }
        let payload = match prepared {
            Ok(payload) => payload,
            Err(FormError::Validation(errors)) => {
                for (key, error) in errors {
                    if let Some(field) = form.fields.get_mut(&key) {
                        field.error = Some(error);
                    }
                }
                return;
            }
            Err(error) => {
                session.view_error = Some(error.to_string());
                return;
            }
        };
        let client_token = token();
        let scope = session.modal.scope().clone();
        let current = session.modal.stack().last().unwrap();
        let app = current.app_id.clone();
        let origin = current.origin_conversation.clone();
        session.modal.enqueue_with_origin(
            &scope,
            client_token.clone(),
            app,
            origin,
            now_ms(),
            60_000,
        );
        session.busy = true;
        session.view_error = None;
        let fixture_revision = session.fixture.then_some(session.modal.revision());
        (
            Ticket::capture(&shell),
            payload,
            client_token,
            fixture_revision,
        )
    };
    if let Some(revision) = fixture_revision {
        if !crate::fixture::is_fixture() {
            return;
        }
        let mut shell = state.write();
        let session = shell.app_modal.as_mut().unwrap();
        let view = session.modal.stack().last().unwrap().id.clone();
        if users_fixture::record(session, &payload, &client_token) {
            return;
        }
        let response = fixture::submission(session, &payload);
        complete(&mut shell, &view, revision, &client_token, Ok(response));
        return;
    }
    let Some(ticket) = ticket else {
        if let Some(session) = state.write().app_modal.as_mut() {
            session.busy = false;
            session.view_error = Some("Reconnect to submit this form.".into());
        }
        return;
    };
    let result = match api::block_view_submit(
        &ticket.context.client,
        &ticket.context.session,
        &ticket.view_id,
        &client_token,
        &payload,
    ) {
        Ok(request) => ticket
            .context
            .transport
            .execute_block_interaction(request)
            .await
            .and_then(|v| api::decode_block_view_submit(&v)),
        Err(error) => Err(error),
    };
    let mut shell = state.write();
    if !ticket.current(&shell) {
        ticket.settle_lost_authority(&mut shell, true);
        return;
    }
    complete(
        &mut shell,
        &ticket.view_id,
        ticket.revision,
        &client_token,
        result,
    );
    drop(shell);
    load_if_needed(state);
}

pub(super) fn complete(
    shell: &mut ShellState,
    view: &str,
    revision: u64,
    client_token: &str,
    result: Result<
        api::BlockViewResponse<api::BlockViewSubmission>,
        super_platinum_core::slack::Error,
    >,
) {
    let Some(session) = shell.app_modal.as_mut() else {
        return;
    };
    let scope = session.modal.scope().clone();
    if session.modal.revision() != revision
        || session.modal.stack().last().is_none_or(|v| v.id != view)
    {
        return;
    }
    session.busy = false;
    match result {
        Ok(api::BlockViewResponse::Rejected(error)) => {
            session.modal.reject(&scope, client_token);
            if let Some(form) = session.current_form_mut() {
                form.apply_server_errors(&error.errors);
            }
            session.view_error = Some(error.view_error.or(error.message).unwrap_or_else(|| {
                "The app could not accept this form. Review your entries.".into()
            }));
        }
        Ok(api::BlockViewResponse::Accepted(response)) => {
            if session
                .modal
                .apply_submission(&scope, view, revision, &response.action)
            {
                session.generation = next_id();
                session.uncertain = false;
                session.conflict = false;
                session.view_error = None;
                if let api::BlockViewResponseAction::Update(payload) = &response.action {
                    if let Some(form) = session.forms.get_mut(view) {
                        if let Err(error) = form.reconcile_submitted(payload) {
                            session.conflict = true;
                            session.view_error = Some(error.to_string());
                        }
                    }
                } else {
                    session.reset_revision = next_id();
                }
                session.sync_forms();
                if session.modal.stack().is_empty() {
                    shell.overlay = None;
                }
                if let Some(message) = response.toast_message {
                    shell.show_toast(message);
                }
            } else {
                session.uncertain = true;
                session.view_error = Some(
                    "The app returned an unexpected form. Your submission will not be repeated."
                        .into(),
                );
            }
        }
        Err(super_platinum_core::slack::Error::Api(code))
            if code == "block_interaction_not_sent_offline" =>
        {
            session.modal.reject(&scope, client_token);
            session.view_error =
                Some("Reconnect to submit this form. It has not been sent.".into());
        }
        Err(_) => {
            session.uncertain = true;
            session.view_error = Some(
                "We could not confirm the app's response. Your submission will not be repeated."
                    .into(),
            );
        }
    }
}

pub(super) async fn close(state: Signal<ShellState>, ticket: CloseTicket) {
    {
        let shell = state.read();
        if !ticket.context.current(&shell) || scope(&shell).as_ref() != Some(&ticket.scope) {
            return;
        }
    }
    let Ok(request) = api::block_view_close(
        &ticket.context.client,
        &ticket.context.session,
        &ticket.view_id,
        &ticket.token,
        ticket.root.as_deref(),
    ) else {
        return;
    };
    // Local removal deduplicates close. No retry or reopening after failure.
    let _ = ticket
        .context
        .transport
        .execute_block_interaction(request)
        .await;
}

#[cfg(test)]
mod ownership_tests;
