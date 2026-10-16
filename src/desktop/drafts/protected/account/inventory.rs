//! One Account's retained Editor/open/operation membership and desktop state.
//!
//! This is a sampled component observation. It does not fence later mutation,
//! attest nested core StorageJobs, enumerate renderer-root futures/native input,
//! or jointly cover Source-open, gaps, Discovery, journal and Baseline.
//! Initialization custody is sampled separately within this component.
//! It must never construct Current, a joint receipt, or a close certificate.
use super::super::{editor, editor_open, local_writer, operation, ownership};
use super::Account;
use std::sync::{Arc, Weak};

pub(crate) struct Inventory {
    account: Weak<Account>,
    initialization: super::initialization::Observation,
    editors_stamp: ownership::Stamp,
    operations_stamp: ownership::Stamp,
    slots: Vec<Slot>,
    operations: Vec<Operation>,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    owned: Option<(Arc<super::super::owned_route::Route>, bool)>,
}
struct Slot {
    // Text identifies a diagnostic slot only. Validation binds allocation stamps.
    _id: String,
    editor: Option<Editor>,
    opening: Option<Opening>,
}
struct Editor {
    original: Arc<editor::Editor>,
    state: ownership::Stamp,
    serial: u64,
    released: Option<(Arc<local_writer::LocalWriter>, ownership::Stamp)>,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    write_slot: ownership::Stamp,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    write: Option<(Arc<super::super::owned_editor::Write>, ownership::Stamp)>,
}
struct Opening {
    original: Arc<editor_open::Job>,
    evidence: ownership::Stamp,
}
struct Operation {
    original: operation::Operation,
    evidence: ownership::Stamp,
    live: bool,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    send: Option<(Arc<super::super::owned_send::Send>, ownership::Stamp)>,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    release: Option<(Arc<super::super::owned_release::Release>, ownership::Stamp)>,
}
impl Account {
    pub(crate) fn ownership_inventory(self: &Arc<Self>) -> Result<Inventory, ownership::Error> {
        let initialization = self.initialization.observe(self)?;
        let (editors_stamp, entries) = self.editors.observe(|editors| {
            editors
                .entries
                .iter()
                .map(|(id, entry)| (id.clone(), entry.editor.clone(), entry.opening.clone()))
                .collect::<Vec<_>>()
        })?;
        let (operations_stamp, operations) = self
            .operations
            .observe(|records| records.ownership_entries())?;
        // No registry lock is held while observing a child. Original Arcs are
        // retained even if removal races enumeration; validation then refuses.
        let slots = entries
            .into_iter()
            .map(|(id, editor, opening)| {
                Ok(Slot {
                    _id: id,
                    editor: editor.map(Editor::capture).transpose()?,
                    opening: opening
                        .map(|original| {
                            let (evidence, ()) = original.evidence.observe(|_| ())?;
                            Ok::<_, ownership::Error>(Opening { original, evidence })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, ownership::Error>>()?;
        let operations = operations
            .into_iter()
            .map(Operation::capture)
            .collect::<Result<_, _>>()?;
        let inventory = Inventory {
            account: Arc::downgrade(self),
            initialization,
            editors_stamp,
            operations_stamp,
            slots,
            operations,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            owned: self.owned.as_ref().map(|route| {
                (
                    route.clone(),
                    route.unsupported.load(std::sync::atomic::Ordering::Acquire),
                )
            }),
        };
        inventory.validate(self)?;
        Ok(inventory)
    }
}
impl Editor {
    fn capture(original: Arc<editor::Editor>) -> Result<Self, ownership::Error> {
        let (state, (serial, released)) = original
            .state
            .observe(|value| (value.serial, value.released.clone()))?;
        let released = released
            .map(|writer| {
                let stamp = writer.ownership_stamp()?;
                Ok::<_, ownership::Error>((writer, stamp))
            })
            .transpose()?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        let (write_slot, write) = original.owned_write.observe(Clone::clone)?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        let write = write
            .map(|writer| {
                let stamp = writer.ownership_stamp()?;
                Ok::<_, ownership::Error>((writer, stamp))
            })
            .transpose()?;
        Ok(Self {
            original,
            state,
            serial,
            released,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            write_slot,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            write,
        })
    }
    fn validate(&self) -> Result<(), ownership::Error> {
        self.original.state.validate(&self.state)?;
        if let Some((writer, stamp)) = &self.released {
            writer.validate_ownership(stamp)?;
        }
        #[cfg(any(test, feature = "protected-native-fixture"))]
        {
            self.original.owned_write.validate(&self.write_slot)?;
            if let Some((writer, stamp)) = &self.write {
                writer.validate_ownership(stamp)?;
            }
            self.original.owned_write.validate(&self.write_slot)?;
        }
        self.original.state.validate(&self.state)
    }
}
impl Operation {
    fn capture(original: operation::Operation) -> Result<Self, ownership::Error> {
        let live = original.current();
        #[cfg(any(test, feature = "protected-native-fixture"))]
        let (evidence, (send, release)) = original
            .record()
            .evidence
            .observe(|value| (value.owned_send.clone(), value.owned_release.clone()))?;
        #[cfg(not(any(test, feature = "protected-native-fixture")))]
        let (evidence, ()) = original.record().evidence.observe(|_| ())?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        let send = send
            .map(|send| {
                let stamp = send.ownership_stamp()?;
                Ok::<_, ownership::Error>((send, stamp))
            })
            .transpose()?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        let release = release
            .map(|release| {
                let stamp = release.ownership_stamp()?;
                Ok::<_, ownership::Error>((release, stamp))
            })
            .transpose()?;
        Ok(Self {
            original,
            evidence,
            live,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            send,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            release,
        })
    }
    fn validate(&self) -> Result<(), ownership::Error> {
        if self.original.current() != self.live {
            return Err(ownership::Error::Changed);
        }
        self.original.record().evidence.validate(&self.evidence)?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        {
            if let Some((send, stamp)) = &self.send {
                send.validate_ownership(stamp)?;
            }
            if let Some((release, stamp)) = &self.release {
                release.validate_ownership(stamp)?;
            }
        }
        self.original.record().evidence.validate(&self.evidence)?;
        if self.original.current() != self.live {
            return Err(ownership::Error::Changed);
        }
        Ok(())
    }
}
impl Inventory {
    /// A successful return means unchanged at these sampled component checks.
    /// There is no quiescence/fence, and retained failed/unknown work stays work.
    pub(crate) fn validate(&self, account: &Arc<Account>) -> Result<(), ownership::Error> {
        if !Weak::ptr_eq(&self.account, &Arc::downgrade(account)) {
            return Err(ownership::Error::Replaced);
        }
        self.membership(account)?;
        for slot in &self.slots {
            if let Some(editor) = &slot.editor {
                editor.validate()?;
            }
            if let Some(opening) = &slot.opening {
                opening.original.evidence.validate(&opening.evidence)?;
            }
        }
        for operation in &self.operations {
            operation.validate()?;
        }
        self.membership(account)
    }
    fn membership(&self, account: &Arc<Account>) -> Result<(), ownership::Error> {
        self.initialization.validate(account)?;
        #[cfg(any(test, feature = "protected-native-fixture"))]
        match (&self.owned, &account.owned) {
            (None, None) => {}
            (Some((captured, unsupported)), Some(current)) if Arc::ptr_eq(captured, current) => {
                if current
                    .unsupported
                    .load(std::sync::atomic::Ordering::Acquire)
                    != *unsupported
                {
                    return Err(ownership::Error::Changed);
                }
            }
            _ => return Err(ownership::Error::Replaced),
        }
        account.editors.validate(&self.editors_stamp)?;
        account.operations.validate(&self.operations_stamp)
    }
    pub(crate) fn initialization_status(&self) -> super::initialization::Status {
        self.initialization.status
    }
    pub(crate) fn editor_slot_count(&self) -> usize {
        self.slots.len()
    }
    pub(crate) fn operation_count(&self) -> usize {
        self.operations.len()
    }
}

#[cfg(test)]
mod tests;
