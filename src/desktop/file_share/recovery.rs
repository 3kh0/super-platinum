use super::*;
use std::{collections::BTreeMap, path::PathBuf};
use super_platinum_core::file_sharing::{State as ReceiptState, store::Journal};

#[derive(Clone)]
pub(super) struct Storage {
    pub key: admission::Key,
    path: Option<PathBuf>,
}
impl Storage {
    pub fn capture(context: &admission::Context) -> Result<Self, Refusal> {
        // Only the captured, validated capability supplies a private path. No
        // environment alias or synthetic account can choose default storage.
        if let Some(capability) = context.fixture() {
            return Ok(Self {
                key: context.key.clone(),
                path: Some(capability.storage_path().to_owned()),
            });
        }
        if crate::fixture::is_fixture() {
            return Err(Refusal::Storage);
        }
        Ok(Self {
            key: context.key.clone(),
            path: None,
        })
    }
    #[cfg(test)]
    pub fn at(key: admission::Key, path: PathBuf) -> Self {
        Self {
            key,
            path: Some(path),
        }
    }
    pub async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, Refusal> {
        // Downstream core dependencies are not compiled with cfg(test). Refuse
        // its default config path unless this test supplied private Storage.
        #[cfg(test)]
        if self.path.is_none() {
            return Err(Refusal::Storage);
        }
        let storage = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut journal = match storage.path {
                Some(path) => Journal::open(path, &storage.key.account),
                None => Journal::open_default(&storage.key.account),
            }
            .map_err(|_| Refusal::Storage)?;
            action(&mut journal).map_err(|_| Refusal::Storage)
        })
        .await
        .map_err(|_| Refusal::Storage)?
    }
}

/// Bounded process-local backup when durable settlement fails. Neither payload
/// type has Debug. Captured task and replacement ShellState share this custody.
#[derive(Clone)]
pub(super) enum Retained {
    Result(Result<Value, Error>),
    Limited,
}
pub(super) struct Pending {
    pub key: admission::Key,
    pub receipt: Option<Receipt>,
    pub outcome: Option<Retained>,
    pub storage_failed: bool,
    pub settlement: Option<retry::Settlement>,
}
#[derive(Default)]
pub(super) struct Custody {
    pub draft_io: draft_io::Queue,
    pub direct_results: BTreeMap<String, direct_recovery::Held>,
    pub pending: BTreeMap<String, Pending>,
    pub drafts: BTreeMap<(admission::Key, String), drafts::Draft>,
    pub draft_visits: BTreeMap<(admission::Key, String), std::sync::Weak<drafts::Visit>>,
    pub canceled_reads: Vec<(String, policy_owner::Read)>,
    barriers: BTreeMap<admission::Key, (u64, u64)>,
}
impl Custody {
    pub fn reserve(&mut self, key: &admission::Key, id: &str, now: u64) -> Result<(), Refusal> {
        let pending = &self.pending;
        self.barriers.retain(|key, (read, write)| {
            *read > now || *write > now || pending.values().any(|p| &p.key == key)
        });
        if self.pending.values().any(|p| &p.key == key) {
            return Err(Refusal::Busy);
        }
        if self.pending.len() >= 32
            || (!self.barriers.contains_key(key) && self.barriers.len() >= 32)
        {
            return Err(Refusal::Capacity);
        }
        self.check(key, now)?;
        self.barriers.entry(key.clone()).or_default();
        self.pending.insert(
            id.into(),
            Pending {
                key: key.clone(),
                receipt: None,
                outcome: None,
                storage_failed: false,
                settlement: None,
            },
        );
        Ok(())
    }
    pub fn check(&self, key: &admission::Key, now: u64) -> Result<(), Refusal> {
        if self
            .barriers
            .get(key)
            .is_some_and(|(r, w)| *r > now || *w > now)
        {
            Err(Refusal::Cooldown)
        } else {
            Ok(())
        }
    }
    pub fn defer(&mut self, key: &admission::Key, error: &Error, write: bool, now: u64) {
        if let Error::RateLimited { retry_after_secs } = error {
            let until = now.saturating_add(retry_after_secs.unwrap_or(30));
            let pair = self.barriers.entry(key.clone()).or_default();
            if write {
                pair.1 = pair.1.max(until)
            } else {
                pair.0 = pair.0.max(until)
            }
        }
    }
    pub fn retain(&mut self, id: &str, result: &Result<Value, Error>) {
        if let Some(entry) = self.pending.get_mut(id) {
            entry.outcome = Some(match result {
                Ok(raw)
                    if !admission::bounded(
                        raw,
                        super_platinum_core::slack::api::file_share::FILE_SHARE_RESPONSE_BYTES,
                        64,
                    ) =>
                {
                    Retained::Limited
                }
                // Clone before the journal consumes Result. Raw oversized data
                // retains ResponseLimit classification rather than unbounded bytes.
                _ => Retained::Result(result.clone()),
            });
        }
    }
    pub fn release(&mut self, id: &str) {
        self.pending.remove(id);
    }
}
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn open_recovery(mut signal: Signal<ShellState>) {
    {
        let mut shell = signal.write();
        if visible(&shell).is_some_and(uncaptured) {
            shell.show_toast(Refusal::Busy.message());
            return;
        }
    }
    let captured = {
        let mut shell = signal.write();
        let mut state = std::mem::take(&mut shell.file_share);
        state.sync(&shell);
        let context = state.context.clone();
        state.dialog = None;
        state.recovery.open = true;
        state.recovery.loading = true;
        state.recovery.error = None;
        let serial = state.recovery.serial.checked_add(1);
        if let Some(s) = serial {
            state.recovery.serial = s;
        }
        if context.is_none() || serial.is_none() {
            state.recovery.loading = false;
            state.recovery.error = Some(Refusal::Owner.message());
        }
        shell.file_share = state;
        shell.overlay = Some(Overlay::FileShare);
        context.zip(serial)
    };
    let Some((context, serial)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = match Storage::capture(&context) {
            Ok(storage) => {
                let key = context.key.clone();
                storage
                    .run(move |j| {
                        Ok((
                            j.unresolved(&key.team, &key.user)?,
                            j.drafts(&key.team, &key.user)?,
                            j.direct_opens(&key.team, &key.user)?,
                        ))
                    })
                    .await
            }
            Err(e) => Err(e),
        };
        let mut shell = signal.write();
        if !context.current(&shell) || shell.file_share.recovery.serial != serial {
            return;
        }
        let retained = shell
            .file_share
            .custody
            .lock()
            .ok()
            .and_then(|h| h.draft_io.retained().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.visit().key.0 == context.key)
            .collect::<Vec<_>>();
        let known_empty = retained
            .iter()
            .filter_map(durable::pending_claim)
            .cloned()
            .collect::<Vec<_>>();
        let local = retained
            .into_iter()
            .filter(durable::unsaved)
            .collect::<Vec<_>>();
        let captured = local
            .iter()
            .filter_map(|t| {
                durable::captured(t).map(|c| state::CapturedDraft {
                    id: t.id(),
                    file_title: c.title,
                    destination_name: c.destination.map(|d| d.name),
                })
            })
            .collect();
        let recovery = &mut shell.file_share.recovery;
        recovery.loading = false;
        recovery.local = local;
        recovery.captured = captured;
        match result {
            Ok((receipts, drafts, direct_opens)) => {
                recovery.direct_opens = direct_opens;
                recovery.receipts = receipts;
                recovery.drafts = drafts;
            }
            Err(_) => {
                recovery.direct_opens.clear();
                recovery.receipts.clear();
                recovery.drafts.clear();
                recovery.error = Some(
                    "Saved file shares could not be loaded. Retained messages are still available below.",
                );
            }
        }
        for draft in known_empty {
            if !recovery.drafts.iter().any(|d| d == &draft) {
                recovery.drafts.push(draft);
            }
        }
    });
}
pub(crate) fn open_receipt(
    shell: &mut ShellState,
    serial: u64,
    attempt: &str,
) -> Result<String, Refusal> {
    if visible(shell).is_some()
        || !recovery_visible(shell).is_some_and(|r| r.serial == serial && !r.loading)
    {
        return Err(Refusal::Changed);
    }
    let context = admission::Context::capture(shell).ok_or(Refusal::Owner)?;
    if !shell
        .file_share
        .context
        .as_ref()
        .is_some_and(|old| old.current(shell))
    {
        return Err(Refusal::Owner);
    }
    let receipt = shell
        .file_share
        .recovery
        .receipts
        .iter()
        .find(|r| {
            r.attempt_id == attempt
                && r.scope.team == context.key.team
                && r.scope.user == context.key.user
        })
        .cloned()
        .ok_or(Refusal::Changed)?;
    show_receipt(shell, receipt, "saved-file-shares-open".into(), None)
}
pub(crate) fn close_recovery(shell: &mut ShellState, serial: u64) {
    if recovery_visible(shell).is_some_and(|r| r.serial == serial) {
        shell.file_share.recovery.open = false;
        shell.overlay = None;
    }
}
pub(super) fn show_receipt(
    shell: &mut ShellState,
    receipt: Receipt,
    return_focus: String,
    url: Option<String>,
) -> Result<String, Refusal> {
    let id = next_id()?;
    shell.file_share.dialog = Some(Dialog {
        id: id.clone(),
        file_id: receipt.scope.file.clone(),
        file_title: receipt.file_title.clone(),
        note: receipt.intent.note().unwrap_or(&[]).to_vec(),
        destination: Some(Destination {
            id: receipt.scope.destination.clone(),
            name: receipt.destination_name.clone(),
            private: false,
        }),
        phase: Phase::Outcome,
        error: None,
        receipt: Some(receipt),
        return_focus,
        editor_ready: true,
        closing: false,
        saved_draft_stale: false,
        slack_url: url,
        admission: None,
        draft_visit: None,
        durable: None,
        draft_storage: None,
        draft_task: None,
        draft_local: false,
    });
    shell.file_share.recovery.open = false;
    shell.overlay = Some(Overlay::FileShare);
    Ok(id)
}

// UI can render these safe enums; no raw response/debug text is a status label.
pub(crate) fn retained_status(shell: &ShellState, attempt: &str) -> Option<ReceiptState> {
    let context = admission::Context::capture(shell)?;
    let held = shell.file_share.custody.lock().ok()?;
    let pending = held.pending.values().find(|p| {
        p.key == context.key && p.receipt.as_ref().is_some_and(|r| r.attempt_id == attempt)
    })?;
    if !pending.storage_failed {
        return None;
    }
    match pending.outcome.as_ref()? {
        Retained::Result(Ok(raw)) => Some(ReceiptState::Server(
            api::file_share::decode_existing_file_share(raw.clone())
                .disposition()
                .clone(),
        )),
        Retained::Result(Err(Error::RateLimited { retry_after_secs })) => {
            Some(ReceiptState::RateLimited {
                retry_after_secs: *retry_after_secs,
            })
        }
        Retained::Result(Err(Error::Api(code))) if code == "draft_mutation_not_sent_offline" => {
            Some(ReceiptState::NotSent(
                super_platinum_core::file_sharing::NotSent::Offline,
            ))
        }
        Retained::Result(Err(_)) => Some(ReceiptState::Unconfirmed),
        Retained::Limited => Some(ReceiptState::Server(
            api::file_share::FileShareDisposition::Uncertain(
                api::file_share::FileShareUncertainty::ResponseLimit,
            ),
        )),
    }
}
