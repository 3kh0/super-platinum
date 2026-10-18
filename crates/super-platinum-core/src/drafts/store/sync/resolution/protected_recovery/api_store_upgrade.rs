//! Validated owned migration; never an implicit Default-store upgrade.
//! An opaque configured/fixture owner and actual Worker fence are mandatory.
use super::*;
use crate::drafts::store::terminal;
use rusqlite::OpenFlags;
use std::{
    fs::File,
    sync::{Arc, mpsc},
};
#[path = "upgrade_owned.rs"]
mod owned;
#[path = "upgrade_rich.rs"]
mod rich;
#[path = "upgrade_schema.rs"]
mod schema;
#[path = "upgrade_slash.rs"]
mod slash;
#[path = "upgrade_snapshot.rs"]
mod snapshot;
#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;
pub(in crate::drafts::store) use owned::Owned;
use snapshot::Image;
pub(in crate::drafts::store) type Fence<'a> = crate::drafts::store::worker::UpgradeFence<'a>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::drafts::store) enum Class {
    Missing,
    OrdinaryLocal,
    Legacy5,
    Protected7,
}
pub(in crate::drafts::store) struct Review {
    owner: Arc<Owned>,
    identity: Option<owned::Identity>,
    _file: Option<File>,
    image: Option<Image>,
    class: Class,
    id: String,
    queue_generation: u64,
}
impl Review {
    pub(in crate::drafts::store) fn class(&self) -> Class {
        self.class
    }
}
#[derive(Debug)]
pub(in crate::drafts::store) struct Receipt {
    pub id: String,
    pub imported: usize,
    pub repeated: bool,
}
// These observations are never durable admission capabilities.
pub(in crate::drafts::store) enum Reply {
    Inspected(Arc<Review>),
    Committed(Receipt),
    Opened(Arc<Review>),
    SlashInitialized { review: String, changed: bool },
}
pub(in crate::drafts::store) enum Action {
    Inspect,
    Commit(Arc<Review>),
    Handoff(Arc<Review>),
    InitializeSlash(Arc<Review>),
}
pub(in crate::drafts::store) struct Command {
    owner: Arc<Owned>,
    action: Action,
    reply: mpsc::SyncSender<Result<Reply, AppError>>,
    #[cfg(any(test, feature = "protected-test-support"))]
    pub after_open: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(any(test, feature = "protected-test-support"))]
    pub before_seal: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(any(test, feature = "protected-test-support"))]
    pub after_seal: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(any(test, feature = "protected-test-support"))]
    pub fault: Option<Step>,
}
#[cfg(any(test, feature = "protected-test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::drafts::store) enum Step {
    Opened,
    Ordinary,
    Imported,
    Adapter,
    History,
    BeforeCommit,
    Commit,
}
impl Command {
    pub(in crate::drafts::store) fn new(
        owner: Arc<Owned>,
        action: Action,
    ) -> (Self, mpsc::Receiver<Result<Reply, AppError>>) {
        let (reply, rx) = mpsc::sync_channel(1);
        (
            Self {
                owner,
                action,
                reply,
                #[cfg(any(test, feature = "protected-test-support"))]
                after_open: None,
                #[cfg(any(test, feature = "protected-test-support"))]
                before_seal: None,
                #[cfg(any(test, feature = "protected-test-support"))]
                after_seal: None,
                #[cfg(any(test, feature = "protected-test-support"))]
                fault: None,
            },
            rx,
        )
    }
    pub(in crate::drafts::store) fn account(&self) -> &str {
        self.owner.account()
    }
    pub(in crate::drafts::store) fn execute(mut self, fence: Fence<'_>) -> terminal::Report {
        let result = self.run(fence);
        let report = terminal::app(&result, |value| match value {
            Reply::Inspected(_) | Reply::Opened(_) => terminal::read(value),
            Reply::Committed(_) => terminal::accepted(value),
            Reply::SlashInitialized { changed, .. } => {
                if *changed {
                    terminal::accepted(value)
                } else {
                    terminal::read(value)
                }
            }
        });
        let _ = self.reply.send(result);
        report
    }
    fn run(&mut self, mut fence: Fence<'_>) -> Result<Reply, AppError> {
        self.owner.check()?;
        fence.check(self.owner.account())?;
        self.owner.bind_worker(fence.worker_id())?;
        if let Action::InitializeSlash(review) = &self.action {
            return self.initialize_slash(review.clone(), fence);
        }
        if let Action::Handoff(review) = &self.action {
            review.validate_handoff(&self.owner, fence.worker_id(), fence.generation())?;
            fence.seal(self.owner.account())?;
            return Ok(Reply::Opened(review.clone()));
        }
        if let Action::Commit(review) = &self.action {
            if !Arc::ptr_eq(&review.owner, &self.owner) {
                return Err(invalid("review belongs to another exclusive owner"));
            }
        }
        let retained = self.owner.retain_db()?;
        let existed = retained.is_some();
        if let Some((file, _)) = &retained {
            self.owner.readonly_preflight(file)?;
        }
        // Inspect never creates a SQLite file, directory, owner row or sidecar.
        let mut conn = if existed {
            Some(Connection::open_with_flags(
                self.owner.path(),
                match self.action {
                    Action::Inspect => OpenFlags::SQLITE_OPEN_READ_ONLY,
                    Action::Commit(_) | Action::Handoff(_) | Action::InitializeSlash(_) => {
                        OpenFlags::SQLITE_OPEN_READ_WRITE
                    }
                },
            )?)
        } else {
            None
        };
        if let Some(conn) = &conn {
            conn.busy_timeout(std::time::Duration::from_millis(100))?;
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        if let Some(hook) = self.after_open.take() {
            hook();
        }
        fence.check(self.owner.account())?;
        self.owner.check()?;
        #[cfg(any(test, feature = "protected-test-support"))]
        step(self.fault, Step::Opened)?;
        if matches!(self.action, Action::Inspect) {
            let image = if let Some(conn) = &conn {
                conn.execute_batch("BEGIN;")?;
                let image = Image::read(conn)?;
                schema::classify(&image, self.owner.account())?;
                conn.execute_batch("COMMIT;")?;
                Some(image)
            } else {
                None
            };
            let class = match &image {
                None => Class::Missing,
                Some(image) => schema::classify(image, self.owner.account())?,
            };
            if self.owner.retain_db()?.as_ref().map(|(_, id)| id)
                != retained.as_ref().map(|(_, id)| id)
            {
                return Err(invalid("database changed during inspection"));
            }
            fence.seal(self.owner.account())?;
            let (file, identity) = match retained {
                Some((f, id)) => (Some(f), Some(id)),
                None => (None, None),
            };
            return Ok(Reply::Inspected(Arc::new(Review {
                owner: self.owner.clone(),
                identity,
                _file: file,
                image,
                class,
                id: uuid::Uuid::new_v4().to_string(),
                queue_generation: fence.generation(),
            })));
        }
        let Action::Commit(review) = &self.action else {
            unreachable!()
        };
        if review.identity.is_some()
            && retained.as_ref().map(|(_, id)| id) != review.identity.as_ref()
        {
            return Err(invalid("reviewed database file replaced"));
        }
        if review.class == Class::Protected7 {
            return Err(invalid(
                "protected store is inspection-only; no repeated activation",
            ));
        }
        let mut created = None;
        let result = (|| {
            if conn.is_none() {
                let file = self.owner.create_db()?;
                created = Some((file, ()));
                conn = Some(Connection::open_with_flags(
                    self.owner.path(),
                    OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?);
            }
            let store = DraftStore(
                conn.take()
                    .ok_or_else(|| invalid("upgrade connection missing"))?,
            );
            store
                .0
                .busy_timeout(std::time::Duration::from_millis(100))?;
            store
                .0
                .execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
            let result = (|| {
                let bound_file = retained
                    .as_ref()
                    .map(|(file, _)| file)
                    .or_else(|| created.as_ref().map(|(file, _)| file))
                    .ok_or_else(|| invalid("missing owned database handle"))?;
                same_file(&self.owner, bound_file)?;
                let tx = adapter_atomic::begin(&store.0)?;
                let current = if existed {
                    Some(Image::read(&tx)?)
                } else {
                    None
                };
                if current != review.image {
                    if let Some(current) = &current {
                        if receipt_matches(current, review)? {
                            schema::classify(current, self.owner.account())?;
                            fence.seal(self.owner.account())?;
                            tx.commit()?;
                            return Ok(Receipt {
                                id: review.id.clone(),
                                imported: imported(review),
                                repeated: true,
                            });
                        }
                    }
                    return Err(invalid("reviewed raw inventory changed"));
                }
                if review.queue_generation.checked_add(1) != Some(fence.generation()) {
                    return Err(invalid("review invalidated by intervening queue work"));
                }
                if let Some(current) = &current {
                    schema::classify(current, self.owner.account())?;
                }
                if current.is_none() {
                    let initial = Image::read(&schema::ordinary()?.0)?;
                    for object in initial.schema {
                        if let Some(sql) = object.sql {
                            if object.name != "sqlite_sequence" {
                                tx.execute_batch(&sql)?;
                            }
                        }
                    }
                } else if current
                    .as_ref()
                    .is_some_and(|v| !v.tables.contains_key("composer_draft_sync"))
                {
                    schema::complete_local_schema(&tx)?;
                }
                #[cfg(any(test, feature = "protected-test-support"))]
                step(self.fault, Step::Ordinary)?;
                storage::activate(&store, self.owner.account())?;
                #[cfg(any(test, feature = "protected-test-support"))]
                step(self.fault, Step::Imported)?;
                adapter_schema::install(&store, self.owner.account())?;
                #[cfg(any(test, feature = "protected-test-support"))]
                step(self.fault, Step::Adapter)?;
                let result = Image::read(&tx)?;
                if let Some(before) = &review.image {
                    before.preservation(&result)?;
                    schema::validate_import(
                        before,
                        result.table("composer_draft_sync")?,
                        self.owner.account(),
                    )?;
                }
                let source = serde_json::to_string(&review.image)?;
                let result = serde_json::to_string(result.table("composer_draft_sync")?)?;
                if source.len() > 2 * 1024 * 1024 || result.len() > 1024 * 1024 {
                    return Err(invalid("migration history bound"));
                }
                tx.execute_batch(schema::HISTORY_SQL)?;
                tx.execute(
                    "INSERT INTO composer_draft_migration_receipt VALUES(1,1,?1,?2,?3,?4)",
                    params![self.owner.account(), review.id, source, result],
                )?;
                #[cfg(any(test, feature = "protected-test-support"))]
                step(self.fault, Step::History)?;
                schema::classify(&Image::read(&tx)?, self.owner.account())?;
                let integrity: String = tx.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
                if integrity != "ok" {
                    return Err(invalid("migration integrity check failed"));
                }
                #[cfg(any(test, feature = "protected-test-support"))]
                if let Some(hook) = self.before_seal.take() {
                    hook();
                }
                self.owner.check()?;
                same_file(&self.owner, bound_file)?;
                #[cfg(any(test, feature = "protected-test-support"))]
                step(self.fault, Step::BeforeCommit)?;
                fence.seal(self.owner.account())?;
                #[cfg(any(test, feature = "protected-test-support"))]
                if let Some(hook) = self.after_seal.take() {
                    hook();
                }
                #[cfg(any(test, feature = "protected-test-support"))]
                if self.fault == Some(Step::Commit) {
                    // Genuine deferred-FK failure at SQLite COMMIT, not an early return.
                    tx.execute_batch("CREATE TABLE upgrade_fault_parent(id INTEGER PRIMARY KEY); CREATE TABLE upgrade_fault_child(id INTEGER REFERENCES upgrade_fault_parent(id) DEFERRABLE INITIALLY DEFERRED); INSERT INTO upgrade_fault_child VALUES(1);")?;
                }
                tx.commit()?;
                Ok(Receipt {
                    id: review.id.clone(),
                    imported: imported(review),
                    repeated: false,
                })
            })();
            drop(store);
            result
        })();
        if result.is_err() && created.is_some() {
            // Only our exclusive create is eligible for removal. No pre-existing
            // file is removed, no recursive delete, and no catch-and-success.
            let cleanup = (|| -> Result<(), AppError> {
                let (file, _) = created.take().unwrap();
                let identity = owned::identity(&file)?;
                drop(file);
                if self.owner.retain_db()?.as_ref().map(|(_, id)| id) != Some(&identity) {
                    return Err(invalid("failed initialization file ownership changed"));
                }
                std::fs::remove_file(self.owner.path())?;
                Ok(())
            })();
            if let Err(cleanup) = cleanup {
                return Err(invalid(&format!(
                    "migration failed: {}; owned initialization cleanup also failed: {cleanup}",
                    result.as_ref().err().unwrap()
                )));
            }
        }
        result.map(Reply::Committed)
    }
}
fn same_file(owner: &Owned, file: &File) -> Result<(), AppError> {
    let id = owned::identity(file)?;
    if owner.retain_db()?.as_ref().map(|(_, actual)| actual) != Some(&id) {
        return Err(invalid("owned database changed before transaction/commit"));
    }
    Ok(())
}
fn imported(review: &Review) -> usize {
    review
        .image
        .as_ref()
        .and_then(|v| v.tables.get("composer_draft_sync"))
        .map_or(0, |t| t.rows.len())
}
#[cfg(any(test, feature = "protected-test-support"))]
fn step(fault: Option<Step>, at: Step) -> Result<(), AppError> {
    if fault == Some(at) {
        Err(invalid("injected upgrade fault"))
    } else {
        Ok(())
    }
}
fn receipt_matches(image: &Image, review: &Review) -> Result<bool, AppError> {
    let Some(table) = image.tables.get("composer_draft_migration_receipt") else {
        return Ok(false);
    };
    schema::validate_history(image, review.owner.account())?;
    let row = &table.rows[0];
    Ok(row[3].text()? == review.id && row[4].text()? == serde_json::to_string(&review.image)?)
}
#[cfg(any(test, feature = "protected-test-support"))]
pub(in crate::drafts::store) fn audit_test_store(
    store: &DraftStore,
    account: &str,
) -> Result<(), AppError> {
    let before = Image::read(&store.0)?;
    if schema::classify(&before, account)? != Class::Protected7 {
        return Err(invalid("expected protected test inventory"));
    }
    if Image::read(&store.0)? != before {
        return Err(invalid("inspection mutated protected inventory"));
    }
    Ok(())
}

#[path = "upgrade_handoff.rs"]
mod handoff;
