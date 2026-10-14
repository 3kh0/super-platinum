//! Drafts are user data, separate from the disposable transcript cache.
use super::{DraftKey, LocalDraft};
use crate::error::AppError;
use rusqlite::{Connection, TransactionBehavior, params};
use std::{collections::BTreeSet, path::Path};

pub mod sync;
pub mod uploads;
mod worker;
pub use worker::{
    LocalWrite, commit_synced, enqueue, load, load_synced, prepare_synced,
    queue_local_if_unchanged, queue_owned_local_if_unchanged, read_local_review,
    reconcile_remote_review, release_synced, save_failed, save_local_alternate,
    save_owned_local_alternate, settle_local_versions,
};
pub type Entries = Vec<(DraftKey, LocalDraft)>;
pub use sync::{LocalAlternate, LocalWriter};

pub struct DraftStore(Connection);
impl DraftStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.execute_batch("PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS composer_drafts (destination TEXT PRIMARY KEY, content TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS composer_draft_sync (
                destination TEXT PRIMARY KEY, storage_revision INTEGER NOT NULL CHECK(storage_revision>0),
                format_version INTEGER NOT NULL, state TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS composer_draft_owner (
                singleton INTEGER PRIMARY KEY CHECK(singleton=1), account TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS composer_draft_blocked (
                destination TEXT PRIMARY KEY, token TEXT NOT NULL, content TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS composer_draft_alternates (
                destination TEXT NOT NULL, writer_id TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK(revision>0), content TEXT NOT NULL,
                PRIMARY KEY(destination,writer_id));
            CREATE TABLE IF NOT EXISTS composer_draft_reviewed_writers (
                destination TEXT NOT NULL, writer_id TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK(revision>0),
                PRIMARY KEY(destination,writer_id));")?;
        uploads::migrate(&conn)?;
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
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (key, draft) in entries {
            if managed(&transaction, &key)? {
                return Err(sync::invalid("synced draft requires an atomic paired save"));
            }
            write_local(&transaction, &key, &draft)?;
        }
        transaction.commit()?;
        Ok(())
    }
    /// Preserve a rejected managed-editor input for explicit recovery. This
    /// never overwrites the paired draft or authorizes an operation dispatch.
    // The worker keeps rejected edits on disk for explicit recovery instead
    // of overwriting enrolled state or endlessly retrying a forbidden write.
    fn save_legacy(&mut self, entries: &Entries) -> Result<BTreeSet<DraftKey>, AppError> {
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut rejected = BTreeSet::new();
        for (key, draft) in entries {
            if managed(&transaction, key)? {
                rejected.insert(key.clone());
                transaction.execute("INSERT INTO composer_draft_blocked(destination,token,content) VALUES(?1,?2,?3) ON CONFLICT(destination) DO UPDATE SET token=excluded.token,content=excluded.content",
                    params![serde_json::to_string(key)?,uuid::Uuid::new_v4().to_string(),serde_json::to_string(draft)?])?;
            } else {
                write_local(&transaction, key, draft)?;
            }
        }
        transaction.commit()?;
        Ok(rejected)
    }
    fn managed_keys(&self) -> Result<BTreeSet<DraftKey>, AppError> {
        let mut statement = self
            .0
            .prepare("SELECT destination FROM composer_draft_sync")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str(&row?)?))
            .collect()
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

fn managed(conn: &Connection, key: &DraftKey) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1)",
        [serde_json::to_string(key)?],
        |row| row.get(0),
    )?)
}
fn write_local(conn: &Connection, key: &DraftKey, draft: &LocalDraft) -> Result<(), AppError> {
    let key = serde_json::to_string(key)?;
    if draft.empty() {
        conn.execute("DELETE FROM composer_drafts WHERE destination=?1", [key])?;
    } else {
        conn.execute("INSERT INTO composer_drafts(destination,content) VALUES(?1,?2) ON CONFLICT(destination) DO UPDATE SET content=excluded.content",
            params![key, serde_json::to_string(draft)?])?;
    }
    Ok(())
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
