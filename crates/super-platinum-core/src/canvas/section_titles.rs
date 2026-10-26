//! Nonpersistent rlfY/Th7G section-rich-text source. Missing is current cache
//! state, never server absence. Callers own account/socket/event authority.
use crate::slack::{
    Error,
    api::{CanvasSectionsRequest, CanvasSectionsResponse},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
const MAX_FILES: usize = 128;
const MAX_ABSENT: usize = 128;

/// Positive/own-undefined entries are bounded. Absent generations exist only
/// while a read or observation holds them; the registry contains weak pointers.
/// Its short synchronous lock performs no I/O or caller callbacks.
#[derive(Default)]
pub struct Store {
    identity: Arc<()>,
    files: BTreeMap<String, Arc<File>>,
    absent: Mutex<BTreeMap<String, Weak<File>>>,
}
impl Clone for Store {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            files: self.files.clone(),
            // Clone is a snapshot: future publications in the clone must not
            // retire the original store's absent witnesses.
            absent: Mutex::new(self.absent.lock().expect("section witness lock").clone()),
        }
    }
}
struct File {
    rows: BTreeMap<String, Option<Value>>,
    bytes: usize,
}
#[derive(Clone)]
pub struct Read {
    store: Arc<()>,
    source: Arc<File>,
    request: CanvasSectionsRequest,
    used: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct Observation {
    store: Arc<()>,
    source: Arc<File>,
    file: String,
    section: String,
}
fn invalid() -> Error {
    Error::Api("invalid_canvas_section_reference".into())
}
impl Store {
    fn source(&self, file: &str) -> Result<Arc<File>, Error> {
        if let Some(source) = self.files.get(file) {
            return Ok(source.clone());
        }
        let mut absent = self.absent.lock().map_err(|_| invalid())?;
        absent.retain(|_, witness| witness.strong_count() > 0);
        if let Some(source) = absent.get(file).and_then(Weak::upgrade) {
            return Ok(source);
        }
        if absent.len() >= MAX_ABSENT {
            return Err(invalid());
        }
        let source = Arc::new(File {
            rows: BTreeMap::new(),
            bytes: 0,
        });
        absent.insert(file.into(), Arc::downgrade(&source));
        Ok(source)
    }
    fn current(&self, file: &str, source: &Arc<File>) -> bool {
        if let Some(current) = self.files.get(file) {
            return Arc::ptr_eq(current, source);
        }
        self.absent
            .lock()
            .ok()
            .and_then(|rows| rows.get(file).and_then(Weak::upgrade))
            .is_some_and(|current| Arc::ptr_eq(&current, source))
    }
    pub fn capture_read(&self, request: &CanvasSectionsRequest) -> Result<Read, Error> {
        Ok(Read {
            store: self.identity.clone(),
            source: self.source(request.file_id())?,
            request: request.clone(),
            used: Arc::new(AtomicBool::new(false)),
        })
    }
    /// False means retired/already consumed. Foreign request receipts never
    /// publish even when their parameters happen to match.
    pub fn apply(&mut self, read: &Read, response: &CanvasSectionsResponse) -> Result<bool, Error> {
        if !Arc::ptr_eq(&read.request.identity, &response.request.identity) {
            return Err(invalid());
        }
        if !read.current(self) {
            return Ok(false);
        }
        let file = read.request.file_id();
        if response.rows.as_ref().is_some_and(Vec::is_empty) {
            // A no-op does not allocate a positive slot or require spare cache
            // capacity, and does not retire still-current observations.
            return Ok(!read.used.swap(true, Ordering::AcqRel));
        }
        if response.rows.is_none() {
            if read.used.swap(true, Ordering::AcqRel) {
                return Ok(false);
            }
            self.files.remove(file);
            self.absent.lock().map_err(|_| invalid())?.remove(file);
            return Ok(true);
        }
        let mut next = read.source.rows.clone();
        for (id, value) in response.rows.as_ref().expect("positive response") {
            next.insert(id.clone(), value.clone());
        }
        let bytes = serde_json::to_vec(&next).map_err(|_| invalid())?.len();
        let retained_bytes: usize = self
            .files
            .iter()
            .filter(|(id, _)| id.as_str() != file)
            .map(|(_, source)| source.bytes)
            .sum();
        if next.len() > 1000
            || (self.files.len() >= MAX_FILES && !self.files.contains_key(file))
            || bytes > 2 * 1024 * 1024
            || retained_bytes.saturating_add(bytes) > 8 * 1024 * 1024
        {
            return Err(invalid());
        }
        if read.used.swap(true, Ordering::AcqRel) {
            return Ok(false);
        }
        self.files
            .insert(file.into(), Arc::new(File { rows: next, bytes }));
        self.absent.lock().map_err(|_| invalid())?.remove(file);
        Ok(true)
    }
    pub fn observe(&self, file: &str, section: &str) -> Result<Observation, Error> {
        CanvasSectionsRequest::new(file, &[section.to_owned()])?;
        Ok(Observation {
            store: self.identity.clone(),
            source: self.source(file)?,
            file: file.into(),
            section: section.into(),
        })
    }
}
impl Read {
    pub fn current(&self, store: &Store) -> bool {
        !self.used.load(Ordering::Acquire)
            && Arc::ptr_eq(&self.store, &store.identity)
            && store.current(self.request.file_id(), &self.source)
    }
}
impl Observation {
    pub fn current(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.store, &store.identity) && store.current(&self.file, &self.source)
    }
    pub fn file_id(&self) -> &str {
        &self.file
    }
    pub fn section_id(&self) -> &str {
        &self.section
    }
    pub fn has_entry(&self) -> bool {
        self.source.rows.contains_key(&self.section)
    }
    pub fn block(&self) -> Option<&Value> {
        self.source.rows.get(&self.section)?.as_ref()
    }
    /// ON4A skips ensured acquisition for every truthy cached value, not only
    /// a semantically admitted preview.
    pub fn is_cached(&self) -> bool {
        match self.block() {
            None | Some(Value::Null) => false,
            Some(Value::Bool(v)) => *v,
            Some(Value::String(v)) => !v.is_empty(),
            Some(Value::Number(v)) => v.as_f64() != Some(0.0),
            Some(_) => true,
        }
    }
}
#[cfg(test)]
mod tests;
