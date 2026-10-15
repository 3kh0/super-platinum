//! Fixed scalar-only trace. A slot has one writer and a release-published flag.
//! The parent reads the mapping only after process exit; no observer owns data.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::{cell::Cell, marker::PhantomData, rc::Rc, sync::OnceLock};

pub const MAGIC: u64 = 0x5350_454d_4a52_3031;
pub const VERSION: u64 = 1;
pub const CAPACITY: usize = 65_536;
pub const WORDS: usize = 16;
pub const HEADER_WORDS: usize = 12;
pub const BYTES: usize = (HEADER_WORDS + CAPACITY * WORDS) * 8;

pub mod event {
    pub const CONSTRUCTED: u64 = 1;
    pub const OUTER_START: u64 = 2;
    pub const PAYLOAD_START: u64 = 3;
    pub const PAYLOAD_END: u64 = 4;
    pub const OUTER_END: u64 = 5;
    pub const LOADED_FINAL: u64 = 6;
    pub const ACTION_START: u64 = 7;
    pub const ACTION_END: u64 = 8;
    pub const TARGET: u64 = 9;
    pub const BUILD_START: u64 = 10;
    pub const BUILD_RETURN: u64 = 11;
    pub const PUBLISHED: u64 = 12;
    pub const REJECTED: u64 = 13;
    pub const HOLD_ENTER: u64 = 14;
    pub const HOLD_RELEASE: u64 = 15;
    pub const WORKER_WAIT: u64 = 16;
    pub const FAILURE: u64 = 17;
    pub const CUTOFF: u64 = 18;
    pub const REMOTE_FETCH: u64 = 19;
    pub const SCOPE_ENTER: u64 = 20;
    pub const SCOPE_EXIT: u64 = 21;
    pub const CLOSE_REQUESTED: u64 = 22;
    pub const WORKER_ACTIVE: u64 = 23;
}

#[repr(C)]
pub struct Header {
    pub magic: AtomicU64,
    pub version: AtomicU64,
    pub capacity: AtomicU64,
    pub next: AtomicU64,
    pub overflow: AtomicU64,
    pub serial_thread: AtomicU64,
    pub current_action: AtomicU64,
    pub reserved: AtomicU64,
    pub pid: AtomicU64,
    pub nonce_low: AtomicU64,
    pub nonce_high: AtomicU64,
    pub process_ordinal: AtomicU64,
}

#[repr(C)]
pub struct Slot(pub [AtomicU64; WORDS]);

#[repr(C)]
pub struct Layout {
    pub header: Header,
    pub slots: [Slot; CAPACITY],
}

struct Recorder {
    // A mapped view with process lifetime; no payload pointer or strong owner.
    address: usize,
    start: Instant,
}
static RECORDER: OnceLock<Recorder> = OnceLock::new();
// The test-only selection never changes production OnceLock or env admission.
fn with_recorder<R>(run: impl FnOnce(Option<&Recorder>) -> R) -> R {
    #[cfg(test)]
    if let Some(enabled) = TEST_ENABLED.get() {
        let pointer = TEST_RECORDER.get();
        // SAFETY: TestGuard borrows the owning ledger, is !Send, and clears the
        // pointer before its owner can move/drop; callbacks cannot retain refs.
        return run(if enabled && !pointer.is_null() {
            Some(unsafe { &*pointer })
        } else {
            None
        });
    }
    run(RECORDER.get())
}
#[cfg(test)]
thread_local! {
    static TEST_ENABLED:Cell<Option<bool>>=const{Cell::new(None)};
    static TEST_RECORDER:Cell<*const Recorder>=const{Cell::new(std::ptr::null())};
}
#[cfg(test)]
pub(super) fn test_enabled() -> Option<bool> {
    TEST_ENABLED.get()
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Location {
    pub context: u64,
    // 0 unknown, 1 actual write guard held, 2 audited not held.
    pub write: u64,
    pub mount: u64,
}
thread_local! {
    static LOCATION: Cell<Location> = const { Cell::new(Location {context:0,write:0,mount:0}) };
    static BUILD: Cell<u64> = const { Cell::new(0) };
}

pub struct Scope {
    previous: Location,
    active: bool,
    // A scope is strictly lexical on its thread, never a Send/await guard.
    _not_send: PhantomData<Rc<()>>,
}
pub fn scope(context: u64, write: u64, mount: u64) -> Scope {
    let active = with_recorder(|r| r.is_some());
    let previous = if active {
        LOCATION.replace(Location {
            context,
            write,
            mount,
        })
    } else {
        Location::default()
    };
    if active {
        emit(event::SCOPE_ENTER, 0, 0, context);
    }
    Scope {
        previous,
        active,
        _not_send: PhantomData,
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if self.active {
            emit(event::SCOPE_EXIT, 0, 0, self.previous.context);
            LOCATION.set(self.previous);
        }
    }
}

pub struct Building {
    previous: u64,
    generation: u64,
    _not_send: PhantomData<Rc<()>>,
}
pub fn building(generation: u64) -> Building {
    let previous = if generation != 0 {
        BUILD.replace(generation)
    } else {
        0
    };
    emit(event::BUILD_START, generation, 0, 0);
    Building {
        previous,
        generation,
        _not_send: PhantomData,
    }
}
impl Drop for Building {
    fn drop(&mut self) {
        emit(event::BUILD_RETURN, self.generation, 0, 0);
        if self.generation != 0 {
            BUILD.set(self.previous);
        }
    }
}
pub fn build_id() -> u64 {
    BUILD.get()
}

/// Caller guarantees alignment, a zeroed initialized header, and a mapped view
/// that remains valid until the process exits. No mapped bytes contain pointers.
pub unsafe fn install(
    address: usize,
    serial_thread: u64,
    nonce: [u64; 2],
    process: u64,
) -> Result<(), String> {
    if address == 0 || address % std::mem::align_of::<Layout>() != 0 {
        return Err("Invalid trace alignment".into());
    }
    let layout = unsafe { &*(address as *const Layout) };
    if layout.header.magic.load(Ordering::Acquire) != MAGIC
        || layout.header.version.load(Ordering::Acquire) != VERSION
        || layout.header.capacity.load(Ordering::Acquire) != CAPACITY as u64
        || layout.header.next.load(Ordering::Acquire) != 0
        || layout.header.overflow.load(Ordering::Acquire) != 0
        || layout.header.pid.load(Ordering::Acquire) != std::process::id() as u64
        || layout.header.nonce_low.load(Ordering::Acquire) != nonce[0]
        || layout.header.nonce_high.load(Ordering::Acquire) != nonce[1]
        || layout.header.process_ordinal.load(Ordering::Acquire) != process
    {
        return Err("Trace mapping header mismatch".into());
    }
    layout
        .header
        .serial_thread
        .store(serial_thread, Ordering::Release);
    RECORDER
        .set(Recorder {
            address,
            start: Instant::now(),
        })
        .map_err(|_| "Duplicate trace mapping".to_string())
}

fn thread_id() -> u64 {
    #[cfg(target_os = "windows")]
    {
        unsafe { windows::Win32::System::Threading::GetCurrentThreadId() as u64 }
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

pub fn elapsed() -> u64 {
    with_recorder(|r| {
        r.map_or(0, |r| {
            r.start.elapsed().as_nanos().min(u64::MAX as u128) as u64
        })
    })
}
pub fn emit(kind: u64, generation: u64, child: u64, related: u64) -> u64 {
    with_recorder(|recorder| {
        let Some(recorder) = recorder else {
            return 0;
        };
        let at = recorder.start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        // SAFETY: install requires this process-lifetime mapping and validates it.
        let layout = unsafe { &*(recorder.address as *const Layout) };
        let index = layout.header.next.fetch_add(1, Ordering::Relaxed);
        if index >= CAPACITY as u64 {
            layout.header.overflow.store(1, Ordering::Release);
            return at;
        }
        let location = LOCATION.get();
        let thread = thread_id();
        let serial = layout.header.serial_thread.load(Ordering::Relaxed);
        // 2 means observed nonserial OS thread; it does NOT assert blocking-worker role.
        let role = if thread == 0 {
            0
        } else if thread == serial {
            1
        } else {
            2
        };
        let words = [
            0,
            at,
            kind,
            generation,
            child,
            location.context,
            thread,
            role,
            location.write,
            location.mount,
            layout.header.current_action.load(Ordering::Acquire),
            related,
            0,
            0,
            0,
            0,
        ];
        let slot = &layout.slots[index as usize];
        for (cell, value) in slot.0.iter().zip(words).skip(1) {
            cell.store(value, Ordering::Relaxed);
        }
        slot.0[0].store(index + 1, Ordering::Release);
        at
    })
}

pub fn action(id: u64) -> u64 {
    with_recorder(|r| {
        if let Some(recorder) = r {
            let layout = unsafe { &*(recorder.address as *const Layout) };
            layout.header.current_action.store(id, Ordering::Release);
            emit(event::ACTION_START, 0, 0, id)
        } else {
            0
        }
    })
}

pub fn generation(account: u64, ticket: u64, remote: bool) -> u64 {
    if account > 3 || ticket == 0 || ticket >= (1u64 << 46) {
        emit(event::FAILURE, 0, 0, 1);
        return 0;
    }
    (account << 48) | (ticket << 2) | if remote { 2 } else { 1 }
}
pub fn scalar_id(value: &str) -> u64 {
    if !super::enabled() {
        return 0;
    }
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
pub(super) struct TestLedger {
    storage: Vec<AtomicU64>,
    recorder: Box<Recorder>,
}
#[cfg(test)]
impl TestLedger {
    pub(super) fn new() -> Self {
        assert_eq!(std::mem::size_of::<Layout>(), BYTES);
        assert_eq!(
            std::mem::align_of::<Layout>(),
            std::mem::align_of::<AtomicU64>()
        );
        let storage: Vec<_> = (0..HEADER_WORDS + CAPACITY * WORDS)
            .map(|_| AtomicU64::new(0))
            .collect();
        for (i, value) in [
            MAGIC,
            VERSION,
            CAPACITY as u64,
            0,
            0,
            thread_id(),
            0,
            0,
            std::process::id() as u64,
            0,
            0,
            1,
        ]
        .into_iter()
        .enumerate()
        {
            storage[i].store(value, Ordering::Relaxed);
        }
        let recorder = Box::new(Recorder {
            address: storage.as_ptr() as usize,
            start: Instant::now(),
        });
        Self { storage, recorder }
    }
    pub(super) fn activate(&self, enabled: bool) -> TestGuard<'_> {
        TestGuard {
            recorder: TEST_RECORDER.replace(&*self.recorder),
            enabled: TEST_ENABLED.replace(Some(enabled)),
            location: LOCATION.replace(Location::default()),
            build: BUILD.replace(0),
            _owner: PhantomData,
            _not_send: PhantomData,
        }
    }
    pub(super) fn words(&self) -> Vec<u64> {
        self.storage
            .iter()
            .map(|x| x.load(Ordering::Acquire))
            .collect()
    }
    pub(super) fn elapsed(&self) -> u64 {
        self.recorder.start.elapsed().as_nanos() as u64
    }
}
#[cfg(test)]
pub(super) struct TestGuard<'a> {
    recorder: *const Recorder,
    enabled: Option<bool>,
    location: Location,
    build: u64,
    _owner: PhantomData<&'a TestLedger>,
    _not_send: PhantomData<Rc<()>>,
}
#[cfg(test)]
impl Drop for TestGuard<'_> {
    fn drop(&mut self) {
        TEST_RECORDER.set(self.recorder);
        TEST_ENABLED.set(self.enabled);
        LOCATION.set(self.location);
        BUILD.set(self.build);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_binary_layout_and_scalar_generation() {
        assert_eq!(std::mem::size_of::<Header>(), HEADER_WORDS * 8);
        assert_eq!(std::mem::size_of::<Slot>(), WORDS * 8);
        assert_eq!(std::mem::size_of::<Layout>(), BYTES);
        assert_ne!(generation(0, 1, false), generation(0, 1, true));
        assert_ne!(generation(0, 1, false), generation(1, 1, false));
        assert_eq!(generation(4, 1, false), 0);
    }
    #[test]
    fn build_context_restores_without_retaining_an_owner() {
        assert_eq!(build_id(), 0);
        {
            let _a = building(5);
            assert_eq!(build_id(), 5);
            {
                let _b = building(9);
                assert_eq!(build_id(), 9);
            }
            assert_eq!(build_id(), 5);
        }
        assert_eq!(build_id(), 0);
    }
    #[test]
    fn scoped_test_recorder_uses_actual_drop_and_restores_disabled_state() {
        let recorder = TestLedger::new();
        assert_eq!(test_enabled(), None);
        {
            let _arm = recorder.activate(true);
            let observed = {
                let _build = building(5);
                std::sync::Arc::new(super::super::payload(Vec::<u8>::new(), 1))
            };
            let final_reader = observed.clone();
            drop(observed);
            assert_eq!(recorder.words()[3], 3); // build start, construction, build return
            drop(final_reader);
            let words = recorder.words();
            assert_eq!(words[3], 7);
            let kinds: Vec<_> = (0..7)
                .map(|i| words[HEADER_WORDS + i * WORDS + 2])
                .collect();
            assert_eq!(kinds, [10, 1, 11, 2, 3, 4, 5]);
            let before = words[3];
            {
                let _disabled = recorder.activate(false);
                assert!(!super::super::enabled());
                drop(super::super::payload(Vec::<u8>::new(), 1));
            }
            assert_eq!(recorder.words()[3], before);
        }
        assert_eq!(test_enabled(), None);
    }
}
