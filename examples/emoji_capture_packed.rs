//! Pure matched snapshot experiment. No runtime hooks, native UI or HTTP.
use serde_json::{Value, json};
use slack::models::Emoji;
use std::{collections::HashMap, hint::black_box, sync::Arc, time::Instant};
use super_platinum_core::{
    composer_emoji::{self, insertion, search, standard},
    slack,
};
#[path = "../crates/super-platinum-core/src/composer_emoji/catalog.rs"]
mod candidate_catalog;
use candidate_catalog::{Catalog, CustomRecord};
#[path = "emoji_capture_packed/build.rs"]
mod build;
#[path = "emoji_capture_packed/checks.rs"]
mod checks;
#[path = "emoji_capture_packed/current.rs"]
mod current;
#[path = "emoji_capture_packed/fixture.rs"]
mod fixture;
#[path = "emoji_capture_packed/packed.rs"]
mod packed;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PickerChoice {
    name: String,
    glyph: String,
    url: Option<String>,
    aliases: String,
}
struct Built {
    catalog: Arc<Catalog>,
    picker: Arc<Vec<PickerChoice>>,
}
enum Snapshot {
    Current(Vec<CustomRecord>),
    Packed(packed::Packed),
}
#[derive(Clone, Copy)]
enum Mode {
    Current,
    Packed,
}
impl Mode {
    fn key(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Packed => "packed",
        }
    }
    fn capture(self, source: &HashMap<String, Emoji>) -> Snapshot {
        match self {
            Self::Current => Snapshot::Current(current::capture(source)),
            Self::Packed => Snapshot::Packed(
                packed::Packed::capture(
                    source.values().map(|e| (e.name.as_str(), e.value.as_str())),
                )
                .expect("fallible pack failed; no ticket exists"),
            ),
        }
    }
}
fn ms(at: Instant) -> f64 {
    at.elapsed().as_secs_f64() * 1000.0
}
fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let at = Instant::now();
    let value = black_box(f());
    (value, ms(at))
}
fn pipeline(source: &HashMap<String, Emoji>, mode: Mode) -> (Arc<Built>, Value) {
    let total = Instant::now();
    // Sizing, reserve and packing are all inside this capture interval.
    let (snapshot, capture_ms) = time(|| mode.capture(black_box(source)));
    let (retained, mut records, decode_ms) = match snapshot {
        Snapshot::Current(records) => (None, records, 0.0),
        Snapshot::Packed(p) => {
            let (records, ms) = time(|| p.decode().expect("validated private snapshot"));
            (Some(p), records, ms)
        }
    };
    // These constant-time capacity reads are included in end-to-end timing;
    // no scan is added. Individual String capacities are not measured.
    let packed_len = retained.as_ref().map_or(0, |p| p.len_bytes());
    let packed_capacity = retained.as_ref().map_or(0, |p| p.capacity_bytes());
    let record_capacity = records.capacity() * std::mem::size_of::<CustomRecord>();
    // Snapshot remains live through complete decode, then is freed before build.
    let (_, captured_buffer_release_ms) = time(|| drop(black_box(retained)));
    let (built, build_picker_ms) = time(|| {
        let (catalog, picker) = build::build(std::mem::take(&mut records)).unwrap();
        Arc::new(Built { catalog, picker })
    });
    let total_ms = ms(total);
    (
        built,
        json!({"capture_ms":capture_ms,"decode_ms":decode_ms,"captured_buffer_release_ms":captured_buffer_release_ms,
        "build_picker_ms":build_picker_ms,"total_ms":total_ms,
        "overlap_capacity_not_rss":{"packed_len":packed_len,"packed_capacity":packed_capacity,"record_vector_capacity":record_capacity}}),
    )
}
fn check(source: &HashMap<String, Emoji>) {
    let baseline = current::capture(source);
    let packed =
        packed::Packed::capture(source.values().map(|e| (e.name.as_str(), e.value.as_str())))
            .unwrap();
    let decoded = packed.decode().unwrap();
    assert_eq!(baseline, decoded);
    let (a, pa) = build::build(baseline).unwrap();
    let (b, pb) = build::build(decoded).unwrap();
    checks::assert_same(&a, &b);
    assert_eq!(*pa, *pb);
}
fn sample(
    a: &HashMap<String, Emoji>,
    b: &HashMap<String, Emoji>,
    mode: Mode,
    round: usize,
) -> Value {
    let (old, initial) = pipeline(a, mode);
    let mut published = Some(old);
    let reader = published.as_ref().unwrap().clone();
    assert_eq!(Arc::strong_count(&reader), 2);
    let (next, replacement) = pipeline(b, mode);
    assert!(!Arc::ptr_eq(&reader, &next));
    if a.len() >= 16 {
        for index in [0, 3, 15] {
            let key = fixture::name(index);
            let before = reader.catalog.resolve(&key, None).unwrap();
            let after = next.catalog.resolve(&key, None).unwrap();
            assert_eq!(before.shortcode, after.shortcode);
            assert_ne!(before.image_url, after.image_url);
            assert_eq!(before.usage_id, after.usage_id);
        }
    }
    let (displaced, publish_ms) = time(|| published.replace(next).unwrap());
    let (_, shared_release_ms) = time(|| drop(black_box(displaced)));
    assert_eq!(Arc::strong_count(&reader), 1);
    assert_eq!(Arc::strong_count(&reader.catalog), 1);
    assert_eq!(Arc::strong_count(&reader.picker), 1);
    let names = reader.catalog.names.len();
    let choices = reader.picker.len();
    let (_, last_old_ms) = time(|| drop(black_box(reader)));
    assert_eq!(Arc::strong_count(published.as_ref().unwrap()), 1);
    let (_, last_replacement_ms) = time(|| drop(black_box(published.take())));
    let (unpublished, unpublished_capture_ms) = time(|| mode.capture(black_box(a)));
    let (_, unpublished_drop_ms) = time(|| drop(black_box(unpublished)));
    json!({"mode":mode.key(),"round":round,"warmup":round==0,"names":names,"picker_choices":choices,
        "initial":initial,"replacement":replacement,"publish_ms":publish_ms,"shared_release_ms":shared_release_ms,
        "last_old_ms":last_old_ms,"last_replacement_ms":last_replacement_ms,
        "unpublished_capture_ms":unpublished_capture_ms,"unpublished_drop_ms":unpublished_drop_ms})
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: emoji_capture_packed <rounds1..100> <seed0..2>"
    );
    let rounds: usize = args[1].parse().unwrap();
    let seed: usize = args[2].parse().unwrap();
    assert!((1..=100).contains(&rounds) && seed < 3);
    let sizes = [0, 1000, 16000, 64000];
    let mut inventories = Vec::new();
    for offset in 0..sizes.len() {
        let count = sizes[(offset + seed) % sizes.len()];
        let a = fixture::corpus(count, 0);
        let b = fixture::corpus(count, 1);
        let before = fixture::inventory(&a);
        let after = fixture::inventory(&b);
        let (_, preflight_ms) = time(|| {
            check(&a);
            check(&b)
        });
        let mut pairs = Vec::new();
        for round in 0..=rounds {
            let order = if (seed + round) % 2 == 0 {
                [Mode::Current, Mode::Packed]
            } else {
                [Mode::Packed, Mode::Current]
            };
            let samples: Vec<_> = order
                .into_iter()
                .map(|m| sample(&a, &b, m, round))
                .collect();
            pairs.push(json!({"round":round,"order":order.map(Mode::key),"samples":samples}));
        }
        assert_eq!(fixture::inventory(&a), before);
        assert_eq!(fixture::inventory(&b), after);
        inventories.push(json!({"count":count,"before":before,"after":after,"preflight_ms":preflight_ms,"pairs":pairs}));
    }
    println!(
        "{}",
        json!({"schema":1,"protocol":"emoji-packed-capture-v1","word_bytes":std::mem::size_of::<usize>(),"record_bytes":std::mem::size_of::<CustomRecord>(),"rounds":rounds,"seed":seed,"retained_warmup_rounds":1,
        "preflight":"Exact records, Catalog output and picker rows for both generations","inventories":inventories})
    );
}

#[cfg(test)]
mod experiment_tests {
    use super::*;
    #[test]
    fn pathological_ordered_records_preserve_catalog_and_picker() {
        for version in [standard::Version::V16, standard::Version::V17] {
            let entries = [
                ("rocket", "https://example.invalid/rocket.png"),
                ("duplicate", "https://example.invalid/a.png"),
                ("duplicate", "alias:rocket"),
                ("chain", "alias:duplicate"),
                ("cycle_a", "alias:cycle_b"),
                ("cycle_b", "alias:cycle_a"),
                ("dangling", "alias:absent"),
                ("invalid:name", "https://example.invalid/x"),
                ("bad_url", "javascript:x"),
                ("", ""),
                ("İΣς🦊\0", "https://example.invalid/x"),
            ];
            let control: Vec<_> = entries
                .iter()
                .map(|(n, v)| CustomRecord {
                    name: (*n).into(),
                    value: (*v).into(),
                })
                .collect();
            let p = packed::Packed::capture(entries.into_iter()).unwrap();
            let decoded = p.decode().unwrap();
            assert_eq!(control, decoded);
            let a = Catalog::build_records(version, control.clone()).unwrap();
            let b = Catalog::build_records(version, decoded.clone()).unwrap();
            checks::assert_same(&a, &b);
            let (a, pa) = build::build(control).unwrap();
            let (b, pb) = build::build(decoded).unwrap();
            checks::assert_same(&a, &b);
            assert_eq!(*pa, *pb);
        }
    }
}
