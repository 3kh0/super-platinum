//! Durable forwarding identities outlive the dialog and active account.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::forwarding::{
    Deliveries,
    store::{Begin, Journal, Receipt, Scope},
};

#[derive(Clone)]
pub(super) struct Storage {
    account: String,
    team: String,
    user: String,
    fixture_path: Option<std::path::PathBuf>,
}
impl Storage {
    pub(super) fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        Some(Self {
            account: shell.core.active_account.clone()?,
            user: shell.core.workspaces.get(&team)?.self_user_id.clone(),
            team,
            fixture_path: if crate::fixture::is_fixture() {
                Some(std::env::var_os("SUPER_PLATINUM_FORWARD_RECOVERY_DB")?.into())
            } else {
                None
            },
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.user)
    }
    pub(super) async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, String> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut journal = if let Some(path) = storage.fixture_path {
                Journal::open(path, &storage.account)
            } else {
                Journal::open_default(&storage.account)
            }
            .map_err(|_| {
                "Saved forwards could not be opened. Try again before forwarding.".to_owned()
            })?;
            action(&mut journal).map_err(|_| {
                "The forwarding record could not be saved. Check delivery before trying again."
                    .to_owned()
            })
        })
        .await
        .map_err(|_| "Saved forwards could not be checked. Please try again.".to_owned())?
    }
    pub(super) async fn source(&self, target: &ForwardTarget) -> Result<Vec<Receipt>, String> {
        let (team, user, source, ts) = (
            self.team.clone(),
            self.user.clone(),
            target.channel.clone(),
            target.message.ts.clone(),
        );
        self.run(move |j| j.list_source(&team, &user, &source, &ts))
            .await
    }
    pub(super) async fn unresolved(&self) -> Result<Vec<Receipt>, String> {
        let (team, user) = (self.team.clone(), self.user.clone());
        self.run(move |j| j.unresolved(&team, &user)).await
    }
    pub(super) async fn begin(
        &self,
        request: api::ForwardMessage,
        name: String,
    ) -> Result<Begin, String> {
        let scope = Scope {
            team_id: self.team.clone(),
            user_id: self.user.clone(),
            source: request.source.clone(),
            timestamp: request.timestamp.clone(),
            destination: request.destination.clone(),
        };
        self.run(move |j| j.begin(scope, name, request, delivery::now()))
            .await
    }
    pub(super) async fn resend(&self, receipt: Receipt) -> Result<Option<Begin>, String> {
        self.run(move |j| j.resend(&receipt.scope, &receipt.attempt_id, delivery::now()))
            .await
    }
    pub(super) async fn refresh(&self, receipt: &Receipt) -> Result<Option<Receipt>, String> {
        let scope = receipt.scope.clone();
        self.run(move |j| j.get(&scope)).await
    }
    pub(super) async fn finish(
        &self,
        receipt: Receipt,
        result: Result<api::ForwardedMessage, super_platinum_core::slack::Error>,
    ) -> Result<Option<Receipt>, String> {
        self.run(move |j| {
            j.finish(&receipt.scope, &receipt.attempt_id, result, delivery::now())?;
            j.get(&receipt.scope)
        })
        .await
    }
    pub(super) async fn confirm(
        &self,
        receipt: Receipt,
        messages: Vec<super_platinum_core::slack::models::Message>,
    ) -> Result<Option<Receipt>, String> {
        self.run(move |j| {
            j.confirm(
                &receipt.scope,
                &receipt.attempt_id,
                &receipt.delivery.request.client_msg_id,
                &receipt.scope.user_id,
                &messages,
                delivery::now(),
            )?;
            j.get(&receipt.scope)
        })
        .await
    }
    pub(super) async fn defer(
        &self,
        receipt: Receipt,
        seconds: u64,
    ) -> Result<Option<Receipt>, String> {
        self.run(move |j| {
            j.defer_check(
                &receipt.scope,
                &receipt.attempt_id,
                delivery::now(),
                delivery::now().saturating_add(seconds),
            )?;
            j.get(&receipt.scope)
        })
        .await
    }
    pub(super) async fn retire(&self, receipt: Receipt) -> Result<Option<Receipt>, String> {
        self.run(move |j| {
            j.retire(&receipt.scope, &receipt.attempt_id, delivery::now())?;
            j.get(&receipt.scope)
        })
        .await
    }
}

fn notes_differ(receipts: &BTreeMap<String, Receipt>) -> bool {
    let mut pending = receipts.values().filter(|r| r.unresolved());
    let Some(first) = pending.next() else {
        return false;
    };
    pending.any(|r| r.delivery.request.blocks != first.delivery.request.blocks)
}

#[derive(Clone, Default)]
pub(super) struct Saved {
    pub loaded: bool,
    pub load_failed: bool,
    mixed_at_load: bool,
    pub storage: Option<Storage>,
    pub receipts: BTreeMap<String, Receipt>,
}
impl Saved {
    pub fn mixed_notes(&self) -> bool {
        let mut pending = self.receipts.values().filter(|r| r.unresolved());
        if self.mixed_at_load && pending.clone().next().is_some() {
            return true;
        }
        let Some(first) = pending.next() else {
            return false;
        };
        pending.any(|r| r.delivery.request.blocks != first.delivery.request.blocks)
    }
    pub fn fixture() -> Self {
        Self {
            loaded: crate::fixture::is_fixture()
                && std::env::var_os("SUPER_PLATINUM_FORWARD_RECOVERY_DB").is_none(),
            ..Self::default()
        }
    }
}

pub(super) async fn load(
    state: Signal<ShellState>,
    target: ForwardTarget,
    mut saved: Signal<Saved>,
    mut ledger: Signal<Deliveries>,
    mut error: Signal<String>,
) {
    if saved.try_peek().ok().is_none_or(|s| s.loaded) {
        return;
    }
    let storage = state.try_peek().ok().and_then(|s| Storage::capture(&s));
    let result = match storage.as_ref() {
        Some(storage) => match super::recovery_check::seed(storage, &target).await {
            Ok(()) => storage.source(&target).await,
            Err(error) => Err(error),
        },
        None => Err("Saved forwards are unavailable for this account.".into()),
    };
    if !state
        .try_peek()
        .ok()
        .is_some_and(|s| same_dialog(&s, &target))
    {
        return;
    }
    match result {
        Ok(receipts) => {
            let receipts = receipts
                .into_iter()
                .filter(|r| {
                    !r.retired && (r.unresolved() || r.delivery.retry_at() > delivery::now())
                })
                .map(|r| (r.scope.destination.clone(), r))
                .collect::<BTreeMap<_, _>>();
            if let Ok(mut value) = ledger.try_write() {
                *value = Deliveries(
                    receipts
                        .iter()
                        .map(|(id, r)| (id.clone(), r.delivery.clone()))
                        .collect(),
                );
            }
            if let Ok(mut value) = saved.try_write() {
                *value = Saved {
                    loaded: true,
                    load_failed: false,
                    mixed_at_load: notes_differ(&receipts),
                    storage,
                    receipts,
                };
            }
        }
        Err(message) => {
            if let Ok(mut value) = saved.try_write() {
                value.load_failed = true;
            }
            if let Ok(mut value) = error.try_write() {
                *value = message;
            }
        }
    }
}

#[cfg(test)]
mod tests;
