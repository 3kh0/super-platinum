//! Title-only file edits with fresh ownership/revision checks and positive receipts.
use super::*;
use super_platinum_core::operation_receipts::{Operation, Outcome};

#[derive(Clone)]
pub struct Edit {
    pub original: String,
    pub draft: String,
}
impl Edit {
    pub fn new(title: &str) -> Self {
        Self {
            original: title.into(),
            draft: title.into(),
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn start(state: Signal<ShellState>, target: Target, mut ui: Signal<ActionState>) {
    let mut action = ui.write();
    if action.busy || action.blocked {
        return;
    }
    let Some(edit) = action.rename.clone() else {
        return;
    };
    let title = match policy::rename_title(&edit.draft) {
        Ok(title) => title.to_owned(),
        Err(message) => {
            action.error = Some(message.into());
            return;
        }
    };
    if action.retry_at > now() {
        action.error = Some(format!(
            "Slack asked us to wait. Try again in {} seconds.",
            action.retry_at.saturating_sub(now())
        ));
        return;
    }
    let check_only = action.uncertain;
    if title == edit.original && !check_only {
        return;
    }
    action.busy = true;
    action.error = None;
    drop(action);
    dioxus::core::spawn_forever(save(state, target, edit.original, title, ui, check_only));
}

pub(super) fn reconcile(shell: &mut ShellState, target: &Target, file: &File) {
    if !target.current(shell)
        || shell
            .core
            .workspaces
            .get(&target.team)
            .is_some_and(|ws| ws.deleted_files.contains(&target.id))
    {
        return;
    }
    crate::file_events::invalidate(shell, &target.team, &target.id);
    crate::file_events::reconcile_metadata(shell, &target.team, file);
}

async fn success(
    mut state: Signal<ShellState>,
    target: &Target,
    file: &File,
    mut ui: Signal<ActionState>,
    read_revision: u64,
    pending: Option<&recovery::Pending>,
) {
    if !recovery::resolve(state, target, ui, pending, Outcome::Confirmed, 0).await
        || !target.current(&state.peek())
    {
        return;
    }
    let mut shell = state.write();
    if !crate::file_events::changed_since(&shell, &target.team, &target.id, read_revision) {
        reconcile(&mut shell, target, file);
    }
    shell.show_toast("File renamed.");
    if let Ok(mut action) = ui.try_write() {
        *action = ActionState::default();
    }
}

#[derive(Debug, PartialEq)]
enum Revision {
    Applied,
    Original,
    Conflict,
}
fn revision(file: &File, original: &str, title: &str) -> Revision {
    let current = file_title(file);
    if current == title {
        Revision::Applied
    } else if current == original {
        Revision::Original
    } else {
        Revision::Conflict
    }
}

fn rejected(error: &Error) -> bool {
    matches!(
        error,
        Error::TransportNotConfigured | Error::RateLimited { .. }
    ) || matches!(error, Error::Api(code) if matches!(code.as_str(),
            "cant_edit_file" | "edit_not_allowed" | "file_not_found" | "missing_scope" |
            "not_authed" | "invalid_auth" | "token_revoked" | "invalid_arguments" | "invalid_title"))
}

async fn save(
    mut state: Signal<ShellState>,
    target: Target,
    original: String,
    title: String,
    mut ui: Signal<ActionState>,
    check_only: bool,
) {
    if !target.current(&state.peek()) {
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        finish(ui, "Reconnect to Slack to rename this file.", check_only);
        return;
    };
    let mut pending = recovery::pending(state, &target, ui);
    if let Some(previous) = pending.as_ref() {
        let Some(current) = recovery::refresh(state, &target, ui, previous).await else {
            return;
        };
        pending = Some(current);
        if !ctx.current(&state.peek()) {
            return;
        }
    }
    let read_revision = crate::file_events::snapshot_revision(&state.peek());
    let fresh = api::fetch_file(&ctx.transport, &ctx.client, &ctx.session, &target.id).await;
    if !ctx.current(&state.peek()) {
        if let Ok(file) = &fresh
            && !policy::deleted(file)
            && revision(file, &original, &title) == Revision::Applied
        {
            recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await;
        } else if let Err(error) = &fresh {
            recovery::cooldown(state, &target, ui, pending.as_ref(), error).await;
        }
        return;
    }
    if let Err(error) = &fresh
        && recovery::cooldown(state, &target, ui, pending.as_ref(), error).await
    {
        return;
    }
    if crate::file_events::changed_since(&state.peek(), &target.team, &target.id, read_revision) {
        finish(
            ui,
            "This file changed while its name was being checked. Check again to review the latest name.",
            check_only,
        );
        return;
    }
    let fresh = match fresh {
        Ok(file) if !policy::deleted(&file) => file,
        Err(Error::Api(code)) if code == "file_deleted" => {
            removed(state, &target);
            finish(
                ui,
                "This file was deleted. The earlier rename cannot be confirmed; you can stop tracking it.",
                check_only,
            );
            return;
        }
        Err(_) => {
            finish(
                ui,
                "This file could not be checked. Your name has been kept; check your connection and access.",
                check_only,
            );
            return;
        }
        Ok(_) => {
            removed(state, &target);
            finish(
                ui,
                "This file was deleted. The earlier rename cannot be confirmed; you can stop tracking it.",
                check_only,
            );
            return;
        }
    };
    if revision(&fresh, &original, &title) == Revision::Applied {
        success(state, &target, &fresh, ui, read_revision, pending.as_ref()).await;
        return;
    }
    if revision(&fresh, &original, &title) == Revision::Conflict {
        reconcile(&mut state.write(), &target, &fresh);
        finish(
            ui,
            if check_only {
                "This file has a different name in Slack. The earlier rename is still unconfirmed. Check again later, or stop tracking it after reviewing the file in Slack."
            } else {
                "This file has a different name in Slack. Cancel and reopen Rename to review it before saving."
            },
            check_only,
        );
        if let Ok(mut action) = ui.try_write() {
            action.blocked = !check_only;
        }
        return;
    }
    if check_only {
        // Absence of the new title is not proof that a timed-out write failed.
        finish(
            ui,
            "The new name is not confirmed yet. Check again later or cancel to review the file in Slack.",
            true,
        );
        return;
    }
    if !policy::can_delete(&fresh, &ctx.session.user_id) || !policy::renameable(&fresh) {
        finish(
            ui,
            "Only your uploaded files can be renamed here. Refresh Files to see current permissions.",
            false,
        );
        if let Ok(mut action) = ui.try_write() {
            action.blocked = true;
        }
        return;
    }
    let request = match api::file_rename(&ctx.client, &ctx.session, &target.id, &title) {
        Ok(request) => request,
        Err(_) => {
            finish(ui, "Enter a valid file name.", false);
            return;
        }
    };
    let Some(started) = recovery::begin(
        state,
        &target,
        ui,
        Operation::Rename {
            original_title: original.clone(),
            requested_title: title.clone(),
        },
    )
    .await
    else {
        return;
    };
    pending = Some(started);
    if !ctx.current(&state.peek())
        || crate::file_events::changed_since(&state.peek(), &target.team, &target.id, read_revision)
    {
        if !recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Rejected, 0).await {
            return;
        }
        if ctx.current(&state.peek()) {
            finish(
                ui,
                "This file changed while preparing the rename. No rename was sent; review the latest name before trying again.",
                false,
            );
        }
        return;
    }
    let result = ctx.transport.execute(request).await;
    if let Err(error) = &result {
        if rejected(error) {
            let retry_at = match error {
                Error::RateLimited { retry_after_secs } => {
                    now().saturating_add(retry_after_secs.unwrap_or(30))
                }
                _ => 0,
            };
            if !recovery::resolve(
                state,
                &target,
                ui,
                pending.as_ref(),
                Outcome::Rejected,
                retry_at,
            )
            .await
                || !ctx.current(&state.peek())
            {
                return;
            }
            if let Error::RateLimited { retry_after_secs } = error {
                if let Ok(mut action) = ui.try_write() {
                    action.retry_at = now().saturating_add(retry_after_secs.unwrap_or(30));
                }
                finish(
                    ui,
                    "Slack is limiting changes. Wait a moment before trying again.",
                    false,
                );
            } else {
                finish(
                    ui,
                    "Slack did not allow this rename. Your name has been kept.",
                    false,
                );
            }
            return;
        }
    }
    if !ctx.current(&state.peek()) {
        return;
    }
    // Even an OK response is followed by files.info; only an exact current title
    // is presented as confirmed. Offline/timeouts can follow a completed write.
    let read_revision = crate::file_events::snapshot_revision(&state.peek());
    let verified = api::fetch_file(&ctx.transport, &ctx.client, &ctx.session, &target.id).await;
    if !ctx.current(&state.peek()) {
        if let Ok(file) = &verified
            && !policy::deleted(file)
            && revision(file, &original, &title) == Revision::Applied
        {
            recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await;
        } else if let Err(error) = &verified {
            recovery::cooldown(state, &target, ui, pending.as_ref(), error).await;
        }
        return;
    }
    if let Err(error) = &verified
        && recovery::cooldown(state, &target, ui, pending.as_ref(), error).await
    {
        return;
    }
    if !crate::file_events::changed_since(&state.peek(), &target.team, &target.id, read_revision)
        && let Ok(file) = &verified
        && !policy::deleted(file)
        && revision(file, &original, &title) == Revision::Applied
    {
        success(state, &target, file, ui, read_revision, pending.as_ref()).await;
    } else {
        finish(
            ui,
            "The rename could not be confirmed. Check its status before making another change.",
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_responses_do_not_allow_blind_retries() {
        for error in [
            Error::Offline("timeout".into()),
            Error::Transport("reset".into()),
            Error::HttpStatus {
                status: 503,
                retry_after_secs: None,
            },
            Error::Api("unknown_error".into()),
        ] {
            assert!(!rejected(&error));
        }
        assert!(rejected(&Error::RateLimited {
            retry_after_secs: Some(17)
        }));
        assert!(rejected(&Error::Api("cant_edit_file".into())));
    }
    #[test]
    fn status_checks_require_the_exact_expected_title() {
        let mut file = File {
            title: Some("Original".into()),
            ..Default::default()
        };
        assert_eq!(revision(&file, "Original", "New"), Revision::Original);
        file.title = Some("New".into());
        assert_eq!(revision(&file, "Original", "New"), Revision::Applied);
        file.title = Some("Renamed elsewhere".into());
        assert_eq!(revision(&file, "Original", "New"), Revision::Conflict);
    }
}
