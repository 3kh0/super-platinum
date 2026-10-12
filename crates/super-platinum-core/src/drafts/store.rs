//! Drafts are user data, separate from the disposable transcript cache.
use super::{DraftKey, LocalDraft};
use crate::error::AppError;
use rusqlite::{Connection, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Condvar, Mutex, OnceLock, mpsc},
};
type Entries = Vec<(DraftKey, LocalDraft)>;
type Updates = BTreeMap<(String, DraftKey), LocalDraft>;
type Load = (String, mpsc::SyncSender<Result<Entries, AppError>>);

pub struct DraftStore(Connection);
impl DraftStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS composer_drafts (destination TEXT PRIMARY KEY, content TEXT NOT NULL);")?;
        Ok(Self(conn))
    }
    fn open_default(account: &str) -> Result<Self, AppError> {
        if uuid::Uuid::parse_str(account).is_err() {
            return Err(std::io::Error::other("invalid draft account").into());
        }
        let dir = crate::config::data_dir()?;
        std::fs::create_dir_all(&dir)?;
        Self::open(dir.join(format!("drafts-{account}.sqlite")))
    }
    pub fn save(
        &mut self,
        entries: impl IntoIterator<Item = (DraftKey, LocalDraft)>,
    ) -> Result<(), AppError> {
        let transaction = self.0.transaction()?;
        for (key, draft) in entries {
            let key = serde_json::to_string(&key)?;
            if draft.empty() {
                transaction.execute(
                    "DELETE FROM composer_drafts WHERE destination=?1",
                    params![key],
                )?;
            } else {
                transaction.execute("INSERT INTO composer_drafts(destination,content) VALUES(?1,?2) ON CONFLICT(destination) DO UPDATE SET content=excluded.content",params![key,serde_json::to_string(&draft)?])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn load(&self) -> Result<Entries, AppError> {
        let mut statement = self
            .0
            .prepare("SELECT destination,content FROM composer_drafts")?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .map(|row| {
                let (key, draft) = row?;
                Ok((serde_json::from_str(&key)?, serde_json::from_str(&draft)?))
            })
            .collect()
    }
}

#[derive(Default)]
struct Pending {
    updates: Updates,
    loads: Vec<Load>,
    failed: BTreeSet<String>,
}
#[derive(Default)]
struct Worker {
    pending: Mutex<Pending>,
    ready: Condvar,
}
impl Worker {
    fn run(&self) {
        loop {
            let (updates, loads) = {
                let mut pending = self.pending.lock().unwrap();
                while pending.updates.is_empty() && pending.loads.is_empty() {
                    pending = self.ready.wait(pending).unwrap();
                }
                (
                    std::mem::take(&mut pending.updates),
                    std::mem::take(&mut pending.loads),
                )
            };
            let mut accounts: BTreeMap<String, Entries> = BTreeMap::new();
            for ((account, key), draft) in updates {
                accounts.entry(account).or_default().push((key, draft));
            }
            let mut retry = false;
            for (account, entries) in accounts {
                let result = DraftStore::open_default(&account)
                    .and_then(|mut store| store.save(entries.clone()));
                let mut pending = self.pending.lock().unwrap();
                if result.is_ok() {
                    pending.failed.remove(&account);
                } else {
                    retry = true;
                    pending.failed.insert(account.clone());
                    for (key, draft) in entries {
                        pending
                            .updates
                            .entry((account.clone(), key))
                            .or_insert(draft);
                    }
                }
            }
            // A load is a barrier: writes queued before it have been attempted.
            for (account, reply) in loads {
                let result = DraftStore::open_default(&account)
                    .and_then(|store| store.load())
                    .map(|entries| {
                        let mut entries: BTreeMap<_, _> = entries.into_iter().collect();
                        // Preserve unsaved edits across account switches, including
                        // a disk failure whose updates remain queued for retry.
                        for ((owner, key), draft) in &self.pending.lock().unwrap().updates {
                            if owner == &account {
                                if draft.empty() {
                                    entries.remove(key);
                                } else {
                                    entries.insert(key.clone(), draft.clone());
                                }
                            }
                        }
                        entries.into_iter().collect()
                    });
                let _ = reply.send(result);
            }
            if retry {
                let pending = self.pending.lock().unwrap();
                let _ = self
                    .ready
                    .wait_timeout(pending, std::time::Duration::from_secs(1))
                    .unwrap();
            }
        }
    }
}
fn worker() -> &'static Worker {
    static WORKER: OnceLock<Worker> = OnceLock::new();
    WORKER.get_or_init(|| {
        std::thread::Builder::new()
            .name("super-platinum-drafts".into())
            .spawn(|| worker().run())
            .expect("draft worker starts");
        Worker::default()
    })
}
/// No serialization or disk work runs on the UI thread. Repeated edits replace
/// the pending value, bounding the queue by destinations rather than keystrokes.
pub fn enqueue(account: String, entries: BTreeMap<DraftKey, LocalDraft>) {
    if entries.is_empty() {
        return;
    }
    let mut pending = worker().pending.lock().unwrap();
    for (key, draft) in entries {
        pending.updates.insert((account.clone(), key), draft);
    }
    worker().ready.notify_one();
}
/// Call only from the startup/background worker.
pub fn load(account: &str) -> Result<Entries, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker()
        .pending
        .lock()
        .unwrap()
        .loads
        .push((account.into(), sender));
    worker().ready.notify_one();
    receiver
        .recv()
        .map_err(|_| std::io::Error::other("draft worker stopped"))?
}
pub fn save_failed(account: &str) -> bool {
    worker().pending.lock().unwrap().failed.contains(account)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_roundtrip_preserves_unicode_selection_files_and_separate_threads() {
        let path =
            std::env::temp_dir().join(format!("platinum-drafts-{}.sqlite", uuid::Uuid::new_v4()));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1.2".into()),
        };
        let mut composer = crate::ComposerState::with_text("*Draft 🚀*");
        composer.set_selection(1, 6);
        let draft = LocalDraft {
            composer: composer.clone(),
            attachments: vec![crate::domain::ComposerAttachment {
                id: 17,
                path: "/tmp/own-test.png".into(),
                name: "own-test.png".into(),
                bytes: 32,
                uploading: true,
                upload_started: Some(std::time::Instant::now()),
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            }],
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), draft)])
            .unwrap();
        let mut store = DraftStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, key);
        assert_eq!(loaded[0].1.composer, composer);
        assert_eq!(loaded[0].1.attachments[0].id, 17);
        assert!(!loaded[0].1.attachments[0].uploading);
        assert!(loaded[0].1.attachments[0].upload_started.is_none());
        let mut channel = key.clone();
        channel.thread = None;
        store
            .save([(
                channel.clone(),
                LocalDraft {
                    composer: crate::ComposerState::with_text("Channel"),
                    ..Default::default()
                },
            )])
            .unwrap();
        store.save([(key, LocalDraft::default())]).unwrap();
        assert_eq!(store.load().unwrap()[0].0, channel);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn invalid_accounts_cannot_choose_a_storage_path() {
        assert!(DraftStore::open_default("../../another-account").is_err());
        assert!(DraftStore::open_default("").is_err());
    }
}
