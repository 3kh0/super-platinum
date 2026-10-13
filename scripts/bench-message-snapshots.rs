//! Isolated clone-allocation measurement for message action revision snapshots.
//! Uses the existing desktop serde_json rlib; does not build or launch the app.
//! Example (substitute the existing hashed rlib path):
//! rustc --edition=2024 -O scripts/bench-message-snapshots.rs \
//!   -L dependency=target/debug/deps --extern serde_json=target/debug/deps/libserde_json-HASH.rlib \
//!   -o /tmp/bench-message-snapshots && /tmp/bench-message-snapshots
//! Numbers cover snapshot cloning only, excluding capture/validation/DOM work.
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

struct CountingAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
fn record(bytes: usize) {
    if TRACK.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}
fn measure<T: Clone>(label: &str, snapshots: &T, iterations: usize) {
    for _ in 0..100 {
        drop(black_box(snapshots.clone()));
    }
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..iterations {
        drop(black_box(snapshots.clone()));
    }
    let elapsed = start.elapsed();
    TRACK.store(false, Ordering::Relaxed);
    println!(
        "{}",
        json!({"case":label,"iterations":iterations,
        "allocations":ALLOCATIONS.load(Ordering::Relaxed),"allocated_bytes":BYTES.load(Ordering::Relaxed),
        "elapsed_us":elapsed.as_micros()})
    );
}
fn main() {
    // Synthetic, representative app content: 12 text blocks and 5 confirmed
    // buttons. Keys match the production source/core revision constructors.
    let mut blocks: Vec<Value> = (0..12).map(|index| json!({
        "type":"section","block_id":format!("section-{index}"),
        "text":{"type":"mrkdwn","text":"Release planning: review the desktop rollout, documentation, and support handoff. ".repeat(4)}
    })).collect();
    blocks.push(json!({"type":"actions","block_id":"release-actions","elements":(0..5).map(|index|json!({
        "type":"button","action_id":format!("approve-{index}"),"value":format!("release-{index}"),
        "text":{"type":"plain_text","text":"Approve"},"style":"primary",
        "confirm":{"title":{"type":"plain_text","text":"Approve release?"},
            "text":{"type":"mrkdwn","text":"Review *this release* before approving."},
            "confirm":{"type":"plain_text","text":"Approve"},"deny":{"type":"plain_text","text":"Cancel"}}
    })).collect::<Vec<_>>()}));
    let profile = json!({"id":"B_FIXTURE","name":"Release helper","app_id":"A_FIXTURE","team_id":"T_FIXTURE",
        "icons":{"image_72":"https://example.test/avatar.png"}});
    let full = json!({"blocks":blocks,"edited":null,"bot_id":"B_FIXTURE","bot_profile":profile,
        "thread_ts":null,"channel":"C_FIXTURE","subtype":null,"type":"message","user":"U_FIXTURE",
        "function_execution_id":null,"is_ephemeral":null,"app_id":null});
    let wire = json!({"blocks":blocks,"bot_id":"B_FIXTURE","app_id":"A_FIXTURE","app_team_id":"T_FIXTURE",
        "function_execution_id":null,"is_ephemeral":false,"ephemeral_thread_ts":null});
    let core = json!({"blocks":blocks,"edited":null,"bot_id":"B_FIXTURE","bot_profile":profile,
        "thread_ts":null,"function_execution_id":null,"is_ephemeral":null});
    println!(
        "{}",
        json!({"fixture":"13 blocks, 5 confirmed buttons","full_revision_json_bytes":full.to_string().len(),
        "wire_revision_json_bytes":wire.to_string().len(),"core_revision_json_bytes":core.to_string().len(),
        "scope":"snapshot clone allocations only; not application frame timing"})
    );
    let source_before = (full, wire);
    let source_after = (
        Arc::new(source_before.0.clone()),
        Arc::new(source_before.1.clone()),
    );
    let core_after = Arc::new(core.clone());
    measure("source_value_before", &source_before, 10_000);
    measure("source_arc_after", &source_after, 10_000);
    measure("core_value_before", &core, 10_000);
    measure("core_arc_after", &core_after, 10_000);
    assert_eq!(&source_before.0, source_after.0.as_ref());
    assert_eq!(&source_before.1, source_after.1.as_ref());
    assert_eq!(&core, core_after.as_ref());
}
