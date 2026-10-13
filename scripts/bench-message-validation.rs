//! Isolated current-source measurement of message-button validation work.
//! Compile with existing artifacts (no Cargo or app launch):
//! rustc --edition=2024 -O scripts/bench-message-validation.rs -L dependency=target/debug/deps \
//!   --extern serde_json=target/debug/deps/libserde_json-HASH.rlib \
//!   --extern super_platinum_core=target/debug/deps/libsuper_platinum_core-HASH.rlib \
//!   -o /tmp/bench-message-validation
//! Current core validation/state modules are included directly below. Canonical
//! scan and historical revision constructors mirror the pre-optimization code.
//! Both current revision helpers are included directly from production source.
//! This measures a validation portion only, not Dioxus frames or whole-app speed.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};
mod slack {
    pub mod models {
        pub use super_platinum_core::slack::models::Message;
    }
}
#[path = "../crates/super-platinum-core/src/block_actions/validation/revision.rs"]
mod core_revision_current;
#[path = "../crates/super-platinum-core/src/block_actions/message_state.rs"]
mod message_state;
#[path = "../src/desktop/message_buttons/source/revision.rs"]
mod source_revision_current;
#[path = "../crates/super-platinum-core/src/block_actions"]
mod measured_core {
    pub mod validation;
}
use measured_core::validation;
use slack::models::Message;
use validation::{CapturedMessageButton, MessageActionOwner};

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
fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    for _ in 0..5 {
        work();
    }
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    let elapsed = start.elapsed();
    TRACK.store(false, Ordering::Relaxed);
    println!(
        "{}",
        json!({"case":label,"iterations":iterations,"allocations":ALLOCATIONS.load(Ordering::Relaxed),
        "allocated_bytes":BYTES.load(Ordering::Relaxed),"elapsed_us":elapsed.as_micros()})
    );
}
fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    }
}
fn fixture() -> Message {
    let mut blocks:Vec<Value>=(0..12).map(|index|json!({"type":"section","block_id":format!("section-{index}"),
        "text":{"type":"mrkdwn","text":"Release planning: review the desktop rollout, documentation, and support handoff. ".repeat(4)}})).collect();
    blocks.push(json!({"type":"actions","block_id":"release-actions","elements":(0..25).map(|index|json!({
        "type":"button","action_id":format!("approve-{index}"),"value":format!("release-{index}"),
        "text":{"type":"plain_text","text":"Approve"},"style":"primary",
        "confirm":{"title":{"type":"plain_text","text":"Approve release?"},"text":{"type":"mrkdwn","text":"Review *this release* before approving."},
        "confirm":{"type":"plain_text","text":"Approve"},"deny":{"type":"plain_text","text":"Cancel"}}})).collect::<Vec<_>>()}));
    serde_json::from_value(json!({"ts":"1719800000.000001","channel":"C1","user":"U1","bot_id":"B1","type":"message",
        "bot_profile":{"id":"B1","name":"Release helper","app_id":"A1","team_id":"TVALID001","icons":{"image_72":"https://example.test/avatar.png"}},
        "blocks":blocks})).unwrap()
}
// Exact keys from desktop message_buttons/source.rs and core validation.rs.
fn source_revision(m: &Message) -> Value {
    json!({"blocks":m.blocks,"edited":m.edited,"bot_id":m.bot_id,
    "bot_profile":m.bot_profile,"thread_ts":m.thread_ts,"channel":m.channel,"subtype":m.subtype,"type":m.kind,"user":m.user,
    "function_execution_id":m.extra.get("function_execution_id"),"is_ephemeral":m.extra.get("is_ephemeral"),"app_id":m.extra.get("app_id")})
}
fn core_revision(m: &Message) -> Value {
    json!({"blocks":m.blocks,"edited":m.edited,"bot_id":m.bot_id,"bot_profile":m.bot_profile,
    "thread_ts":m.thread_ts,"function_execution_id":m.extra.get("function_execution_id"),"is_ephemeral":m.extra.get("is_ephemeral")})
}
// Mirrors exact_message's duplicate rejection. Pending/deletion membership is
// excluded here; the benchmark never substitutes this helper in production.
fn exact_scan<'a>(bag: &'a [Message], ts: &str) -> Option<&'a Message> {
    let mut found = bag
        .iter()
        .filter(|message| message.ts.as_deref() == Some(ts));
    let message = found.next()?;
    if found.next().is_some() {
        None
    } else {
        Some(message)
    }
}
fn check_equivalence(message: &Message, full: &Value, core: &Value) {
    for case in 0..24 {
        let mut m = message.clone();
        match case {
            0 => {}
            1 => m.blocks[0]["text"]["text"] = json!("Edited"),
            2 => m.blocks.swap(0, 1),
            3 => m.blocks[12]["elements"][0]["value"] = json!("changed"),
            4 => {
                m.blocks[12]["elements"][0]["confirm"]["text"]["text"] =
                    json!("Changed confirmation")
            }
            5 => m.bot_profile = None,
            6 => m.bot_profile.as_mut().unwrap().id = Some("OTHER".into()),
            7 => m.bot_profile.as_mut().unwrap().name = Some("Renamed".into()),
            8 => {
                m.bot_profile
                    .as_mut()
                    .unwrap()
                    .extra
                    .insert("app_id".into(), json!("A2"));
            }
            9 => {
                m.bot_profile
                    .as_mut()
                    .unwrap()
                    .extra
                    .insert("unknown".into(), json!({"nested":[null,1,true]}));
            }
            10 => {
                m.bot_profile
                    .as_mut()
                    .unwrap()
                    .icons
                    .as_mut()
                    .unwrap()
                    .image_72 = Some("https://example.test/changed".into())
            }
            11 => m.edited = Some(json!({"ts":"2.0","user":"U2"})),
            12 => m.bot_id = Some("B2".into()),
            13 => m.thread_ts = Some("1719800000.000000".into()),
            14 => {
                m.extra.insert("function_execution_id".into(), json!("F1"));
            }
            15 => {
                m.extra.insert("is_ephemeral".into(), json!(true));
            }
            16 => m.channel = Some("C2".into()),
            17 => m.subtype = Some("bot_message".into()),
            18 => m.kind = Some("other".into()),
            19 => m.user = Some("U2".into()),
            20 => {
                m.extra.insert("app_id".into(), json!("A2"));
            }
            21 => m.text = Some("Unrelated fallback text".into()),
            22 => {
                m.extra.insert("function_execution_id".into(), Value::Null);
            }
            23 => m.edited = Some(Value::Null),
            _ => unreachable!(),
        }
        assert_eq!(
            source_revision_current::matches(&m, full),
            *full == source_revision(&m),
            "source case {case}"
        );
        assert_eq!(
            core_revision_current::matches(&m, core),
            *core == core_revision(&m),
            "core case {case}"
        );
    }
}
fn main() {
    let message = fixture();
    let owner = owner();
    let full = source_revision_current::capture(&message);
    let core = core_revision_current::capture(&message);
    assert_eq!(full, source_revision(&message));
    assert_eq!(core, core_revision(&message));
    check_equivalence(&message, &full, &core);
    let captured =
        CapturedMessageButton::capture(owner.clone(), &message, "release-actions", "approve-0")
            .unwrap();
    println!(
        "{}",
        json!({"fixture":"13 blocks / 25 confirmed buttons","source_revision_json_bytes":full.to_string().len(),
        "core_revision_json_bytes":core.to_string().len(),"equivalence_cases":24,"scope":"isolated validation portion; not app frame timing"})
    );
    for round in 1..=3 {
        println!("{}", json!({"round":round}));
        measure("source_revision_build_compare", 2000, || {
            black_box(source_revision(black_box(&message)) == full);
        });
        measure("production_source_revision_matches", 2000, || {
            black_box(source_revision_current::matches(
                black_box(&message),
                black_box(&full),
            ));
        });
        measure("core_revision_build_compare", 2000, || {
            black_box(core_revision(black_box(&message)) == core);
        });
        measure("production_core_revision_matches", 2000, || {
            black_box(core_revision_current::matches(
                black_box(&message),
                black_box(&core),
            ));
        });
        measure("production_core_capture", 2000, || {
            black_box(
                CapturedMessageButton::capture(
                    owner.clone(),
                    black_box(&message),
                    "release-actions",
                    "approve-0",
                )
                .unwrap(),
            );
        });
        measure("production_core_revalidate", 2000, || {
            black_box(
                captured
                    .revalidate(black_box(&owner), black_box(&message))
                    .unwrap(),
            );
        });
        measure("production_stateless_scan", 2000, || {
            black_box(message_state::stateless(black_box(&message)).unwrap());
        });
    }
    for size in [100, 1000, 10000] {
        let mut bag: Vec<Message> = (0..size - 1)
            .map(|index| Message {
                ts: Some(format!("1719700000.{index:06}")),
                ..Message::default()
            })
            .collect();
        bag.push(message.clone());
        measure(&format!("canonical_unique_scan_{size}"), 5000, || {
            black_box(exact_scan(black_box(&bag), black_box("1719800000.000001")).unwrap());
        });
        if size != 1000 {
            continue;
        }
        let actions: Vec<String> = (0..25).map(|index| format!("approve-{index}")).collect();
        for round in 1..=3 {
            println!(
                "{}",
                json!({"composite_round":round,"buttons_per_iteration":25,"canonical_messages":1000})
            );
            measure("presentation_validation_portion_baseline", 200, || {
                for action in &actions {
                    let m = exact_scan(black_box(&bag), "1719800000.000001").unwrap();
                    assert!(source_revision(m) == full);
                    message_state::stateless(m).unwrap();
                    black_box(
                        CapturedMessageButton::capture(owner.clone(), m, "release-actions", action)
                            .unwrap(),
                    );
                }
            });
            measure(
                "presentation_validation_portion_production_matches",
                200,
                || {
                    for action in &actions {
                        let m = exact_scan(black_box(&bag), "1719800000.000001").unwrap();
                        assert!(source_revision_current::matches(m, &full));
                        message_state::stateless(m).unwrap();
                        black_box(
                            CapturedMessageButton::capture(
                                owner.clone(),
                                m,
                                "release-actions",
                                action,
                            )
                            .unwrap(),
                        );
                    }
                },
            );
        }
    }
}
