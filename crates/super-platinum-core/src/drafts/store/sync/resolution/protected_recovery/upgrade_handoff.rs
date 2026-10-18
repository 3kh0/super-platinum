//! Methods on the actual inspected object; no deserialized admission certificate.
use super::*;

impl Review {
    pub(in crate::drafts::store) fn validate_handoff(
        &self,
        owner: &Arc<Owned>,
        worker: &str,
        generation: u64,
    ) -> Result<(), AppError> {
        if !Arc::ptr_eq(&self.owner, owner)
            || self.class != Class::Protected7
            || self.queue_generation.checked_add(1) != Some(generation)
            || self._file.is_none()
            || self.identity.is_none()
        {
            return Err(invalid("inspected handoff identity or generation changed"));
        }
        owner.bind_worker(worker)?;
        // A recovery sidecar appearing after Inspect must not cause SQLite to
        // perform recovery while this still-unaccepted handoff is being checked.
        owner.readonly_preflight(self._file.as_ref().unwrap())?;
        let store = self.open_owned(owner, worker)?;
        let current = Image::read(&store.0)?;
        if self.image.as_ref() != Some(&current)
            || schema::classify(&current, owner.account())? != Class::Protected7
        {
            return Err(invalid("protected inspection changed before handoff"));
        }
        Ok(())
    }

    pub(in crate::drafts::store) fn open_owned(
        &self,
        owner: &Arc<Owned>,
        worker: &str,
    ) -> Result<DraftStore, AppError> {
        if !Arc::ptr_eq(&self.owner, owner) || self.class != Class::Protected7 {
            return Err(invalid("protected command belongs to another owned route"));
        }
        owner.check()?;
        owner.bind_worker(worker)?;
        let (file, identity) = owner
            .retain_db()?
            .ok_or_else(|| invalid("owned protected database missing"))?;
        if self.identity.as_ref() != Some(&identity) {
            return Err(invalid("owned protected database replaced"));
        }
        // Never run ordinary DDL, initialize or repair here. The handoff's
        // retained file and exclusive marker remain live throughout dispatch.
        let conn = Connection::open_with_flags(owner.path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(std::time::Duration::from_millis(100))?;
        same_file(owner, &file)?;
        Ok(DraftStore(conn))
    }

    pub(in crate::drafts::store) fn row_count(&self) -> usize {
        self.image.as_ref().map_or(0, |image| {
            image.tables.values().map(|table| table.rows.len()).sum()
        })
    }

    pub(in crate::drafts::store) fn migration_id(&self) -> Option<&str> {
        // Image::shape + classify/validate_history ran before this Review was
        // constructed. Still use checked access; metadata never grants authority.
        self.image
            .as_ref()?
            .tables
            .get("composer_draft_migration_receipt")?
            .rows
            .first()?
            .get(3)?
            .text()
            .ok()
    }

    #[cfg(any(test, feature = "protected-test-support"))]
    pub(in crate::drafts::store) fn fixture_ticket(&self) -> Result<String, AppError> {
        self.owner.fixture_ticket(
            self._file
                .as_ref()
                .ok_or_else(|| invalid("missing handoff file"))?,
        )
    }
}
