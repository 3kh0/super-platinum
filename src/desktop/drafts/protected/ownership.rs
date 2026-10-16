//! Allocation-bound observations of existing desktop owners, not drain authority.
//!
//! Mutable access to the original mutex payload changes its stamp before any
//! mutation. Read locks stay read-only. This avoids a second owner map and makes
//! existing mutation paths participate without relying on callers to tick a
//! parallel counter. Interior-mutability children require their own observation.
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError, TryLockError, TryLockResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Changed,
    Replaced,
    Unknown,
}

pub(crate) struct Tracked<T> {
    identity: Arc<()>,
    inner: Mutex<Value<T>>,
}
struct Value<T> {
    generation: Option<u64>,
    payload: T,
}
/// No public constructor, serialization, or authority-bearing conversion.
pub(super) struct Stamp {
    identity: Arc<()>,
    generation: u64,
}
pub(crate) struct Guard<'a, T> {
    inner: MutexGuard<'a, Value<T>>,
    changed: bool,
}
impl<T: Default> Default for Tracked<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
impl<T> Tracked<T> {
    pub fn new(payload: T) -> Self {
        Self {
            identity: Arc::new(()),
            inner: Mutex::new(Value {
                generation: Some(0),
                payload,
            }),
        }
    }
    /// Preserve the existing mutex poison contract and original payload.
    pub fn lock(&self) -> LockResult<Guard<'_, T>> {
        match self.inner.lock() {
            Ok(inner) => Ok(Guard {
                inner,
                changed: false,
            }),
            Err(error) => Err(PoisonError::new(Guard {
                inner: error.into_inner(),
                changed: false,
            })),
        }
    }
    pub fn try_lock(&self) -> TryLockResult<Guard<'_, T>> {
        match self.inner.try_lock() {
            Ok(inner) => Ok(Guard {
                inner,
                changed: false,
            }),
            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
            Err(TryLockError::Poisoned(error)) => {
                Err(TryLockError::Poisoned(PoisonError::new(Guard {
                    inner: error.into_inner(),
                    changed: false,
                })))
            }
        }
    }
    pub(super) fn observe<R>(&self, read: impl FnOnce(&T) -> R) -> Result<(Stamp, R), Error> {
        let value = self.inner.lock().map_err(|_| Error::Unknown)?;
        let generation = value.generation.ok_or(Error::Unknown)?;
        Ok((
            Stamp {
                identity: self.identity.clone(),
                generation,
            },
            read(&value.payload),
        ))
    }
    pub(super) fn validate(&self, stamp: &Stamp) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.identity, &stamp.identity) {
            return Err(Error::Replaced);
        }
        let value = self.inner.lock().map_err(|_| Error::Unknown)?;
        match value.generation {
            None => Err(Error::Unknown),
            Some(current) if current != stamp.generation => Err(Error::Changed),
            Some(_) => Ok(()),
        }
    }
}
impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner.payload
    }
}
impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        if !self.changed {
            self.inner.generation = self.inner.generation.and_then(|v| v.checked_add(1));
            self.changed = true;
        }
        &mut self.inner.payload
    }
}

#[cfg(test)]
mod tests;
