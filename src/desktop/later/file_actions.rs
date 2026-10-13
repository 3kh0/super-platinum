//! Durable, explicit file-to-Later changes. No mounted component owns a write.
mod guards;
#[cfg(test)]
mod tests;
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{path::PathBuf, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    later::{SavedAction, SavedItem, SavedKey},
    saved_file_receipts::{Action, Begin, FileScope, Journal, Operation, Outcome, Receipt, Status},
    slack::{Error, SlackClient, Transport, api},
};

#[derive(Clone)]
pub struct Target {
    pub id: String,
    pub title: String,
    pub team: String,
    account: String,
    user: String,
    epoch: u64,
    visit: u64,
    transport: Option<Arc<Transport>>,
    path: Option<PathBuf>,
}
impl PartialEq for Target {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.title == other.title
            && self.team == other.team
            && self.account == other.account
            && self.user == other.user
            && self.epoch == other.epoch
            && self.visit == other.visit
            && self.path == other.path
            && match (&self.transport, &other.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
impl Target {
    pub fn capture(shell: &ShellState, id: &str, title: &str) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        let path = if crate::fixture::is_fixture() {
            Some(std::env::var_os("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB")?.into())
        } else {
            None
        };
        Some(Self {
            id: id.into(),
            title: title.into(),
            account: shell.core.active_account.clone()?,
            user: shell.core.workspaces.get(&team)?.self_user_id.clone(),
            team,
            epoch: shell.core.account_epoch,
            visit: shell.draft_writes.view_id,
            transport: shell.core.transport.clone(),
            path,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.visit
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.user)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn scope(&self) -> FileScope {
        FileScope {
            team_id: self.team.clone(),
            user_id: self.user.clone(),
            file_id: self.id.clone(),
        }
    }
    fn key(&self) -> SavedKey {
        SavedKey {
            item_type: "file".into(),
            item_id: self.id.clone(),
            ts: String::new(),
        }
    }
    async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Journal, &FileScope) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, String> {
        let target = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut journal = if let Some(path) = &target.path {
                Journal::open(path, &target.account)
            } else {
                Journal::open_default(&target.account)
            }
            .map_err(|_| "Saved file changes could not be opened.".to_owned())?;
            action(&mut journal, &target.scope())
                .map_err(|_| "The file's Later change could not be saved for recovery.".to_owned())
        })
        .await
        .map_err(|_| "Saved file changes could not be checked.".to_owned())?
    }
}
#[derive(Clone, Default)]
pub struct View {
    pub receipt: Option<Receipt>,
    pub message: Option<&'static str>,
}
fn now() -> u64 {
    super_platinum_core::state::now_secs().max(0) as u64
}
pub async fn load(target: &Target) -> Result<View, String> {
    let mut receipt = target.run(|j, s| j.get(s)).await?;
    if let Some(until) = guards::deadline(target, now())
        && let Some(receipt) = &mut receipt
    {
        receipt.retry_at = receipt.retry_at.max(until);
    }
    Ok(View {
        receipt,
        message: None,
    })
}

async fn cooling_down(target: &Target) -> Result<Option<View>, String> {
    let Some(until) = guards::deadline(target, now()) else {
        return Ok(None);
    };
    // Retry the disk write before another control can issue any HTTP. Failure
    // retains the process barrier, even if the component that saw 429 is gone.
    let persisted = target
        .run(move |j, s| {
            let Some(receipt) = j.get(s)? else {
                return Ok(false);
            };
            if receipt.retry_at >= until {
                return Ok(true);
            }
            j.defer_check(s, &receipt.operation_id, now(), until)
        })
        .await?;
    if persisted {
        guards::persisted(target, until);
    }
    let mut view = load(target).await?;
    view.message = Some("Slack asked us to wait before another change or status check.");
    Ok(Some(view))
}
pub async fn inventory(state: Signal<ShellState>) -> Result<Vec<(Target, View)>, String> {
    let target = Target::capture(&state.peek(), "_", "")
        .ok_or("Connect to a workspace to review saved file changes.")?;
    let receipts = target
        .run(|j, s| j.recoverable(&s.team_id, &s.user_id, now()))
        .await?;
    if !target.current(&state.peek()) {
        return Err("The active workspace changed.".into());
    }
    Ok(receipts
        .into_iter()
        .map(|mut receipt| {
            let mut target = target.clone();
            target.id = receipt.scope.file_id.clone();
            target.title = receipt.operation.title.clone();
            if let Some(until) = guards::deadline(&target, now()) {
                receipt.retry_at = receipt.retry_at.max(until);
            }
            (
                target,
                View {
                    receipt: Some(receipt),
                    message: None,
                },
            )
        })
        .collect())
}

struct Network {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
}
impl Network {
    fn capture(state: &Signal<ShellState>, target: &Target) -> Option<Self> {
        if crate::fixture::is_fixture() {
            return None;
        }
        if !target.current(&state.peek()) {
            return None;
        }
        let (transport, client, sessions) = crate::bootstrap::credentials(state)?;
        let session = sessions
            .into_iter()
            .find(|s| s.team_id == target.team && s.user_id == target.user)?;
        Some(Self {
            transport,
            client,
            session,
        })
    }
}
fn fixture(target: &Target) -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("later-files" | "later-files-narrow")
        )
        && std::env::var("SUPER_PLATINUM_LATER_FILES_CHECK").as_deref() == Ok("1")
        && target.account == "00000000-0000-4000-8000-000000000098"
        && target.team == "T1"
        && target.user == "U0"
        && target.path.as_ref().is_some_and(|p| {
            p.is_absolute()
                && p.file_name()
                    .is_some_and(|n| n == "saved-file-recovery.sqlite")
        })
}
async fn write(
    network: Option<&Network>,
    target: &Target,
    action: SavedAction,
) -> Result<Option<SavedItem>, Error> {
    if fixture(target) {
        if target.id != "FSTART" {
            return Err(Error::Transport("unsupported fixture file".into()));
        }
        if std::env::var("SUPER_PLATINUM_SAVED_FILE_UNCERTAIN").as_deref() == Ok("1") {
            return Err(Error::Transport("fixture unconfirmed response".into()));
        }
        let item = fixture_item(&target.id, action);
        return api::decode_saved_file_mutation(
            serde_json::json!({"ok":true,"item":item}),
            &target.id,
            action,
        );
    }
    let network = network.ok_or(Error::TransportNotConfigured)?;
    api::change_saved_file(
        &network.transport,
        &network.client,
        &network.session,
        &target.id,
        action,
    )
    .await
}
fn fixture_item(id: &str, action: SavedAction) -> SavedItem {
    SavedItem {
        todo_state: if matches!(action, SavedAction::Complete) {
            "completed"
        } else {
            "saved"
        }
        .into(),
        item_type: "file".into(),
        item_id: id.into(),
        state: match action {
            SavedAction::Complete => "completed",
            SavedAction::Archive => "archived",
            _ => "in_progress",
        }
        .into(),
        is_archived: matches!(action, SavedAction::Archive),
        date_completed: if matches!(action, SavedAction::Complete) {
            1
        } else {
            0
        },
        date_due: if let SavedAction::Reminder(due) = action {
            due
        } else {
            0
        },
        ..Default::default()
    }
}
async fn read(network: Option<&Network>, target: &Target) -> Result<Option<SavedItem>, Error> {
    if fixture(target) {
        return Ok((target.id == "FSTART").then(|| fixture_item(&target.id, SavedAction::Save)));
    }
    let network = network.ok_or(Error::TransportNotConfigured)?;
    api::find_saved_file(
        &network.transport,
        &network.client,
        &network.session,
        &target.id,
    )
    .await
}
fn deadline(error: &Error) -> Option<u64> {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => Some(now().saturating_add(retry_after_secs.unwrap_or(30).max(1))),
        _ => None,
    }
}
fn rejected(error: &Error) -> bool {
    matches!(error,Error::Api(code) if matches!(code.as_str(),"invalid_auth"|"not_authed"|"account_inactive"|"file_not_found"|"permission_denied"|"invalid_arguments"|"invalid_arg_name"|"not_allowed_token_type"))
        || matches!(
            error,
            Error::RateLimited { .. }
                | Error::HttpStatus { status: 429, .. }
                | Error::TransportNotConfigured
        )
}

pub async fn change(
    mut state: Signal<ShellState>,
    target: Target,
    action: SavedAction,
) -> Result<View, String> {
    let _active = guards::Active::claim(&target)?;
    let result = change_inner(state, target.clone(), action).await;
    if target.current(&state.peek()) {
        let mut shell = state.write();
        shell.core.later.file_actions_revision =
            shell.core.later.file_actions_revision.wrapping_add(1);
    }
    result
}

async fn change_inner(
    mut state: Signal<ShellState>,
    target: Target,
    action: SavedAction,
) -> Result<View, String> {
    if !target.current(&state.peek()) {
        return Err("The active workspace changed.".into());
    }
    if let Some(view) = cooling_down(&target).await? {
        return Ok(view);
    }
    if matches!(action, SavedAction::Save)
        && state
            .peek()
            .core
            .workspaces
            .get(&target.team)
            .is_some_and(|w| w.deleted_files.contains(&target.id))
    {
        return Err("This file was deleted and cannot be saved to Later.".into());
    }
    let network = Network::capture(&state, &target);
    if network.is_none() && !fixture(&target) {
        return Err("Connect to Slack before changing this file in Later.".into());
    }
    let operation = Operation {
        title: target.title.clone(),
        action: action.into(),
    };
    let begin = target
        .run(move |j, s| j.begin(s.clone(), operation, now()))
        .await?;
    let receipt = match begin {
        Begin::Started(r) => r,
        Begin::Blocked(r) => {
            return Ok(View {
                receipt: Some(r),
                message: Some(
                    "An earlier change or cooldown must be resolved before another change.",
                ),
            });
        }
    };
    if !target.current(&state.peek()) {
        let id = receipt.operation_id.clone();
        target
            .run(move |j, s| j.resolve(s, &id, Outcome::Rejected, now(), 0))
            .await?;
        return Err("The active workspace changed before this change was sent.".into());
    }
    let token = {
        let mut shell = state.write();
        shell.core.later.ensure_scope(target.epoch, &target.team);
        shell.core.later.begin_item_request(target.key())
    };
    let result = write(network.as_ref(), &target, action).await;
    let id = receipt.operation_id.clone();
    let (outcome, retry_at) = match &result {
        Ok(_) => (Some(Outcome::Confirmed), 0),
        Err(error) => (
            rejected(error).then_some(Outcome::Rejected),
            deadline(error).unwrap_or(0),
        ),
    };
    if retry_at > 0 {
        guards::remember(&target, retry_at);
    }
    let saved = if let Some(outcome) = outcome {
        target
            .run(move |j, s| j.resolve(s, &id, outcome, now(), retry_at))
            .await?
    } else if retry_at > 0 {
        target
            .run(move |j, s| j.defer_check(s, &id, now(), retry_at))
            .await?
    } else {
        true
    };
    if !saved {
        return Err("Saved file state changed. Reopen this file to review it.".into());
    }
    if retry_at > 0 {
        guards::persisted(&target, retry_at);
    }
    if target.current(&state.peek())
        && let Ok(item) = &result
        && let Some(token) = token
    {
        state
            .write()
            .core
            .later
            .apply_item_response(&token, item.clone());
    }
    if result.is_ok() && !crate::fixture::is_fixture() {
        let target = target.clone();
        // The mutation is already confirmed and persisted. Refresh counts as
        // independent read work; a failed list refresh cannot undo that result.
        dioxus::core::spawn_forever(async move {
            let filter = {
                let shell = state.peek();
                if !target.current(&shell)
                    || shell.main_view != MainView::Later
                    || !shell.core.later.matches_scope(target.epoch, &target.team)
                {
                    return;
                }
                shell.core.later.filter
            };
            super::load(state, filter, false).await;
        });
    }
    let mut view = load(&target).await?;
    view.message = Some(match result {
        Ok(_) => "Later updated.",
        Err(ref e) if deadline(e).is_some() => {
            "Slack asked us to wait. The saved cooldown applies before another change."
        }
        Err(ref e) if rejected(e) => "Slack did not apply this change.",
        Err(_) => "This change is not confirmed. Check status before making another change.",
    });
    Ok(view)
}

/// Positive state may confirm a goal. Omissions and changed state never prove
/// that an earlier mutation failed; particularly removal cannot use omission.
fn confirms(action: Action, item: &SavedItem) -> bool {
    match action {
        Action::Save => true,
        Action::Remove => false,
        Action::Complete => {
            item.date_completed > 0 || item.state == "completed" || item.todo_state == "completed"
        }
        Action::Archive => item.is_archived || item.state == "archived",
        // These need explicit negative fields; serde defaults are not evidence.
        Action::Restore | Action::Unarchive => false,
        Action::Reminder(due) => item.date_due == due && due > 0,
    }
}
pub async fn check(
    mut state: Signal<ShellState>,
    target: Target,
    operation_id: String,
) -> Result<View, String> {
    let _active = guards::Active::claim(&target)?;
    let result = check_inner(state, target.clone(), operation_id).await;
    if target.current(&state.peek()) {
        let mut shell = state.write();
        shell.core.later.file_actions_revision =
            shell.core.later.file_actions_revision.wrapping_add(1);
    }
    result
}

async fn check_inner(
    mut state: Signal<ShellState>,
    target: Target,
    operation_id: String,
) -> Result<View, String> {
    if !target.current(&state.peek()) {
        return Err("The active workspace changed.".into());
    }
    if let Some(view) = cooling_down(&target).await? {
        return Ok(view);
    }
    let network = Network::capture(&state, &target);
    if network.is_none() && !fixture(&target) {
        return Err("Connect to Slack to check this file's saved status.".into());
    }
    let id = operation_id.clone();
    let Some(receipt) = target
        .run(move |j, s| j.reserve_check(s, &id, now()))
        .await?
    else {
        let mut view = load(&target).await?;
        view.message = Some("Saved state changed or the check cooldown is still active.");
        return Ok(view);
    };
    if !target.current(&state.peek()) {
        return Err("The active workspace changed.".into());
    }
    let token = {
        let mut shell = state.write();
        shell.core.later.ensure_scope(target.epoch, &target.team);
        shell.core.later.begin_item_request(target.key())
    };
    let result = read(network.as_ref(), &target).await;
    if let Err(error) = &result
        && let Some(until) = deadline(error)
    {
        let id = operation_id.clone();
        guards::remember(&target, until);
        if target
            .run(move |j, s| j.defer_check(s, &id, now(), until))
            .await?
        {
            guards::persisted(&target, until);
        }
    }
    let confirmed = matches!(&result,Ok(Some(item)) if confirms(receipt.operation.action,item));
    if confirmed {
        let id = operation_id.clone();
        if !target
            .run(move |j, s| j.resolve(s, &id, Outcome::Confirmed, now(), 0))
            .await?
        {
            return Err("Saved file state changed. Reopen this file to review it.".into());
        }
    }
    let mut view = load(&target).await?;
    if !view.receipt.as_ref().is_some_and(|receipt| {
        receipt.operation_id == operation_id
            && (receipt.status == Status::Uncertain
                || (confirmed && receipt.status == Status::Confirmed))
    }) {
        return Err("Saved file state changed. Reopen this file to review it.".into());
    }
    if target.current(&state.peek())
        && let Ok(Some(item)) = &result
        && let Some(token) = token
    {
        state
            .write()
            .core
            .later
            .apply_item_response(&token, Some(item.clone()));
    }
    view.message = Some(if confirmed {
        "This file's saved status is confirmed."
    } else {
        "This change is still not confirmed. Nothing was retried; missing list entries do not prove removal."
    });
    Ok(view)
}
pub async fn retire(target: &Target, operation_id: String) -> Result<View, String> {
    let _active = guards::Active::claim(target)?;
    let _ = cooling_down(target).await?;
    let changed = target
        .run(move |j, s| {
            if j.retire(s, &operation_id, now())? {
                return Ok(true);
            }
            Ok(j.get(s)?
                .is_some_and(|r| r.operation_id == operation_id && r.status != Status::Uncertain))
        })
        .await?;
    if !changed {
        return Err("Saved file state changed. Reopen this file to review it.".into());
    }
    let mut view = load(target).await?;
    view.message =
        Some("Stopped tracking this change. This does not confirm whether Slack applied it.");
    Ok(view)
}

pub fn change_from_later(mut state: Signal<ShellState>, key: SavedKey, action: SavedAction) {
    let target = {
        let shell = state.peek();
        let title = shell
            .core
            .later
            .files
            .get(&key.item_id)
            .map(super_platinum_core::state::file_title)
            .unwrap_or_else(|| key.item_id.clone());
        Target::capture(&shell, &key.item_id, &title)
    };
    let Some(target) = target else {
        return;
    };
    if !state.write().core.later.busy.insert(key.clone()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = change(state, target.clone(), action).await;
        if !target.current(&state.peek()) {
            return;
        }
        let mut shell = state.write();
        shell.core.later.busy.remove(&key);
        shell.core.later.error = match result {
            Ok(view) => view.message.map(str::to_owned),
            Err(error) => Some(error),
        };
    });
}
