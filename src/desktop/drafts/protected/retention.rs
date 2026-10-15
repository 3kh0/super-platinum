//! A bounded account-owned home for writers after UI/registry retirement.
//! Unresolved input is never evicted. New writer admission stops at the bound.
use super::editor::Editor;
use super::*;
pub(super) const MAX_EDITORS: usize = 64;
pub(super) const MAX_JOBS: usize = 32;
pub(super) const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub(super) fn input_size(local: &LocalDraft) -> Result<(), durable::Error> {
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_INPUT_BYTES - self.0 {
                return Err(std::io::Error::other("input admission limit"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(0), local).map_err(|_| durable::Error::Unsupported)
}
/// One shared slot budget; opening jobs do not allocate a second capacity pool.
#[derive(Default)]
pub(super) struct Editors {
    pub(super) entries: BTreeMap<String, Entry>,
    next: u64,
}
pub(super) struct Entry {
    pub(super) editor: Option<Arc<Editor>>,
    pub(super) opening: Option<Arc<super::editor_open::Job>>,
}
impl Editors {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn values(&self) -> impl Iterator<Item = &Option<Arc<Editor>>> {
        self.entries.values().map(|entry| &entry.editor)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Option<Arc<Editor>>)> {
        self.entries.iter().map(|(id, entry)| (id, &entry.editor))
    }
    pub fn get(&self, id: &str) -> Option<&Option<Arc<Editor>>> {
        self.entries.get(id).map(|entry| &entry.editor)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Option<Arc<Editor>>> {
        self.entries.get_mut(id).map(|entry| &mut entry.editor)
    }
    pub fn remove(&mut self, id: &str) -> Option<Option<Arc<Editor>>> {
        self.entries.remove(id).map(|entry| entry.editor)
    }
    pub(super) fn reserve_id(&mut self) -> Result<String, durable::Error> {
        if self.entries.len() >= MAX_EDITORS {
            return Err(durable::Error::Pending);
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(durable::Error::Unsupported)?;
        let id = format!("protected-{next}");
        if self.entries.contains_key(&id) {
            return Err(durable::Error::Unsupported);
        }
        self.next = next;
        Ok(id)
    }
}
pub(super) struct EditorSlot {
    account: Arc<Account>,
    id: String,
    installed: bool,
}
impl Account {
    pub(super) fn reserve_editor(self: &Arc<Self>) -> Result<EditorSlot, durable::Error> {
        let mut editors = self.editors.lock().map_err(|_| durable::Error::Storage)?;
        let id = editors.reserve_id()?;
        editors.entries.insert(
            id.clone(),
            Entry {
                editor: None,
                opening: None,
            },
        );
        Ok(EditorSlot {
            account: self.clone(),
            id,
            installed: false,
        })
    }
    /// Includes detached failed writers from other visits. These are actual
    /// captured Editors, not reconstructed writers or current-owner authority.
    pub fn retained_editors(&self) -> Result<Vec<Arc<Editor>>, durable::Error> {
        Ok(self
            .editors
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .values()
            .filter_map(Clone::clone)
            .collect())
    }
}
impl EditorSlot {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn install(mut self, editor: Arc<Editor>) -> Result<(), durable::Error> {
        let mut editors = self
            .account
            .editors
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        let slot = editors.get_mut(&self.id).ok_or(durable::Error::Stale)?;
        if slot.is_some() || editor.retention_id != self.id {
            return Err(durable::Error::Stale);
        }
        *slot = Some(editor);
        self.installed = true;
        Ok(())
    }
}
impl Drop for EditorSlot {
    fn drop(&mut self) {
        if !self.installed {
            if let Ok(mut editors) = self.account.editors.lock() {
                editors.remove(&self.id);
            }
        }
    }
}
impl Editor {
    pub fn retained_id(&self) -> &str {
        &self.retention_id
    }
    /// Explicit close only after exact durable acceptance. Failed/pending input
    /// cannot free this slot. A retired Editor rejects later submit calls; a new
    /// mounted writer must be registered normally rather than reviving it.
    pub async fn retire_clean(self: &Arc<Self>) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                editor.flush().await?;
                let _gate = editor.gate.lock().await;
                let before = editor.snapshot();
                if before.pending.is_some()
                    || before.running
                    || before.parked
                    || before.error.is_some()
                    || !before.failed_inputs.is_empty()
                    || before.projection.is_some()
                {
                    return Err(durable::Error::ReviewRequired);
                }
                let current = match before.view.inventory() {
                    Inventory::Managed(_) => Inventory::Managed(editor.owner.review().await?),
                    Inventory::Local(_) => Inventory::Local(editor.owner.local_inventory().await?),
                };
                if !before.view.inventory().same(&current) {
                    return Err(durable::Error::Stale);
                }
                let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                if state.serial != before.serial
                    || state.running
                    || state.busy
                    || state.pending.is_some()
                    || state.error.is_some()
                    || state.retry.is_some()
                {
                    return Err(durable::Error::Stale);
                }
                let mut retained = editor
                    .owner
                    .account
                    .editors
                    .lock()
                    .map_err(|_| durable::Error::Storage)?;
                if retained
                    .get(&editor.retention_id)
                    .and_then(Option::as_ref)
                    .is_none_or(|current| !Arc::ptr_eq(current, &editor))
                {
                    return Err(durable::Error::Stale);
                }
                state.retired = true;
                retained.remove(&editor.retention_id);
                Ok(())
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
}

#[cfg(test)]
mod opening_identity_tests {
    use super::*;
    #[test]
    fn counter_exhaustion_and_collision_refuse_without_replacing_slots() {
        let mut slots = Editors::default();
        slots.next = u64::MAX;
        assert!(matches!(
            slots.reserve_id(),
            Err(durable::Error::Unsupported)
        ));
        assert_eq!(slots.next, u64::MAX);
        assert_eq!(slots.len(), 0);
        slots.next = 0;
        slots.entries.insert(
            "protected-1".into(),
            Entry {
                editor: None,
                opening: None,
            },
        );
        assert!(matches!(
            slots.reserve_id(),
            Err(durable::Error::Unsupported)
        ));
        assert_eq!(slots.next, 0);
        assert_eq!(slots.len(), 1);
        assert!(slots.get("protected-1").unwrap().is_none());
    }
    #[test]
    fn checked_reservations_preserve_opaque_shape_and_never_reuse_account_ids() {
        let mut slots = Editors::default();
        let first = slots.reserve_id().unwrap();
        assert_eq!(first, "protected-1");
        slots.entries.insert(
            first.clone(),
            Entry {
                editor: None,
                opening: None,
            },
        );
        let second = slots.reserve_id().unwrap();
        assert_eq!(second, "protected-2");
        slots.entries.remove(&first);
        assert_eq!(slots.reserve_id().unwrap(), "protected-3");
        assert_ne!(first, second);
        // Another Account has its own namespace. The numeric text alone does
        // not confer allocation ownership (editor_open::belongs_to tests it).
        assert_eq!(Editors::default().reserve_id().unwrap(), first);
    }
}
