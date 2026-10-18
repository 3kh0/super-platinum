//! Additive Hosted schema initialization under an exact pre-handoff inspection.
use super::*;

impl Command {
    pub(super) fn initialize_hosted(
        &mut self,
        review: Arc<Review>,
        mut fence: Fence<'_>,
    ) -> Result<Reply, AppError> {
        if !Arc::ptr_eq(&self.owner, &review.owner) || review.class != Class::Protected7 {
            return Err(invalid(
                "hosted initialization requires owned Protected7 review",
            ));
        }
        let (file, identity) = self
            .owner
            .retain_db()?
            .ok_or_else(|| invalid("reviewed database missing"))?;
        if review.identity.as_ref() != Some(&identity) {
            return Err(invalid("reviewed database replaced"));
        }
        self.owner.readonly_preflight(&file)?;
        let conn =
            Connection::open_with_flags(self.owner.path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(std::time::Duration::from_millis(100))?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        #[cfg(any(test, feature = "protected-test-support"))]
        if let Some(hook) = self.after_open.take() {
            hook();
        }
        self.owner.check()?;
        fence.check(self.owner.account())?;
        same_file(&self.owner, &file)?;
        #[cfg(any(test, feature = "protected-test-support"))]
        step(self.fault, Step::Opened)?;
        let tx = adapter_atomic::begin(&conn)?;
        let before = Image::read(&tx)?;
        if review.image.as_ref() != Some(&before)
            || review.queue_generation.checked_add(1) != Some(fence.generation())
            || schema::classify(&before, self.owner.account())? != Class::Protected7
        {
            return Err(invalid("hosted schema review or queue generation changed"));
        }
        hosted_files::initialize_transaction(&tx, self.owner.account())?;
        #[cfg(any(test, feature = "protected-test-support"))]
        step(self.fault, Step::Adapter)?;
        let after = Image::read(&tx)?;
        preserve(&before, &after)?;
        if schema::classify(&after, self.owner.account())? != Class::Protected7 {
            return Err(invalid("initialized schema is not exact protected schema"));
        }
        let integrity: String = tx.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(invalid("hosted initialization integrity failed"));
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        if let Some(hook) = self.before_seal.take() {
            hook();
        }
        self.owner.check()?;
        same_file(&self.owner, &file)?;
        #[cfg(any(test, feature = "protected-test-support"))]
        step(self.fault, Step::BeforeCommit)?;
        fence.seal(self.owner.account())?;
        #[cfg(any(test, feature = "protected-test-support"))]
        if let Some(hook) = self.after_seal.take() {
            hook();
        }
        #[cfg(any(test, feature = "protected-test-support"))]
        if self.fault == Some(Step::Commit) {
            tx.execute_batch("CREATE TABLE upgrade_fault_parent(id INTEGER PRIMARY KEY); CREATE TABLE upgrade_fault_child(id INTEGER REFERENCES upgrade_fault_parent(id) DEFERRABLE INITIALLY DEFERRED); INSERT INTO upgrade_fault_child VALUES(1);")?;
        }
        tx.commit()?;
        Ok(Reply::HostedInitialized {
            review: review.id.clone(),
            changed: before != after,
        })
    }
}

fn preserve(before: &Image, after: &Image) -> Result<(), AppError> {
    if before.user_version != after.user_version
        || before.application_id != after.application_id
        || before
            .schema
            .iter()
            .any(|object| !after.schema.contains(object))
    {
        return Err(invalid("hosted initialization changed existing schema"));
    }
    for (name, old) in &before.tables {
        if after.tables.get(name) != Some(old) {
            return Err(invalid("hosted initialization changed retained raw rows"));
        }
    }
    Ok(())
}
