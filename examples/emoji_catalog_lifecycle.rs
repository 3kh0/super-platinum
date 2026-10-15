//! Copy only this file to examples/emoji_catalog_lifecycle.rs for a root-owned
//! locked example run. The exact reviewed Catalog source stays in tmp.
use super_platinum_core::{
    composer_emoji::{insertion, search, standard},
    slack,
};
#[path = "../tmp/emoji-completion-integration-prep/core/catalog.rs"]
mod catalog;

use catalog::Catalog;
use serde_json::{Value, json};
use slack::models::Emoji;
use std::{collections::HashMap, hint::black_box, sync::Arc, time::Instant};

const VERSION: &str = "emoji-owned-catalog-lifecycle-v1";
const SIZES: &[usize] = &[0, 1_000, 16_000, 64_000];

fn name(index: usize) -> String {
    format!("workspace_release_train_{index:06}_celebration_custom_asset")
}

fn corpus(count: usize, generation: usize) -> HashMap<String, Emoji> {
    (0..count).map(|index| {
        let name = name(index);
        let value = if index % 4 == 3 {
            let target = if index % 16 == 15 { index - 4 } else { index - 3 + generation % 3 };
            format!("alias:{}", self::name(target))
        } else {
            // Synthetic, never fetched. Similar path/name lengths to workspace
            // emoji plus a deliberately retained cache-variant query string.
            format!("https://emoji-fixture.invalid/T_SYNTHETIC_ACCOUNT_{generation}/team-shared/emoji/{name}/{:016x}{:016x}{:016x}{:016x}/original_128x128_animated_asset.gif?version={generation}&content_variant=workspace_original&cache_generation={generation:08}",
                index as u64, index as u64 ^ 0xa1b2c3d4e5f60718, index as u64 ^ 0x1967deadbeef2048, 0x1234abcdef567890u64)
        };
        let extra = [
            ("source".into(), json!({"team":format!("T_SYNTHETIC_ACCOUNT_{generation}"),"asset":{"width":128,"height":128,"animated":index%2==0}})),
            ("labels".into(), json!([format!("release_train_{}_workspace_custom_celebration",index%97),"retained_unknown_metadata_for_full_clone_ownership"])),
            ("provenance".into(), json!({"generation":generation,"record":index,"creator":"U_SYNTHETIC_FIXTURE_CREATOR"})),
        ].into();
        let row = Emoji {name:name.clone(),value,updated:Some(1_700_000_000 + index as u64 + generation as u64),extra};
        (name,row)
    }).collect()
}

fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn capture(source: &HashMap<String, Emoji>) -> (Vec<Emoji>, f64) {
    let at = Instant::now();
    // Exact current desktop capture expression; includes every owned field.
    let snapshot: Vec<_> = black_box(source).values().cloned().collect();
    let milliseconds = elapsed(at);
    (black_box(snapshot), milliseconds)
}

fn assert_owned(source: &HashMap<String, Emoji>, snapshot: &[Emoji]) {
    assert_eq!(source.len(), snapshot.len());
    for row in snapshot {
        let original = &source[&row.name];
        assert_eq!(row.name, original.name);
        assert_eq!(row.value, original.value);
        assert_eq!(row.updated, original.updated);
        assert_eq!(row.extra, original.extra);
        assert_ne!(row.name.as_ptr(), original.name.as_ptr());
        assert_ne!(row.value.as_ptr(), original.value.as_ptr());
    }
}

fn construct(mut snapshot: Vec<Emoji>) -> (Arc<Catalog>, f64, f64) {
    let at = Instant::now();
    // Exact desktop deterministic fallback after the HashMap loses source order.
    snapshot.sort_by(|a, b| a.name.cmp(&b.name));
    let sort_ms = elapsed(at);
    let at = Instant::now();
    let catalog = Arc::new(Catalog::build(standard::Version::V16, black_box(snapshot)).unwrap());
    let build_ms = elapsed(at);
    (black_box(catalog), sort_ms, build_ms)
}

struct Published {
    owner: usize,
    catalog: Arc<Catalog>,
}

fn sample(a: &HashMap<String, Emoji>, b: &HashMap<String, Emoji>, round: usize) -> Value {
    let (snapshot, capture_initial_ms) = capture(a);
    assert_owned(a, &snapshot);
    let (old, sort_initial_ms, build_initial_ms) = construct(snapshot);
    let mut published = Some(Published {
        owner: 0,
        catalog: old,
    });
    let reader = published.as_ref().unwrap().catalog.clone();
    assert_eq!(Arc::strong_count(&reader), 2);
    let catalog_names = reader.names.len();
    let catalog_groups = reader.search_entries.len();

    // Full replacement capture/build takes place while both old publication
    // and an outstanding old-owner reader are live, as during account change.
    let (snapshot, capture_replacement_ms) = capture(b);
    assert_owned(b, &snapshot);
    let (next, sort_replacement_ms, build_replacement_ms) = construct(snapshot);
    assert!(!Arc::ptr_eq(&reader, &next));
    assert_eq!(Arc::strong_count(&reader), 2);
    if a.len() >= 16 {
        for index in [0, 3, 15] {
            let key = name(index);
            let before = reader.resolve(&key, None).unwrap();
            let after = next.resolve(&key, None).unwrap();
            assert_eq!(before.shortcode, after.shortcode);
            assert_ne!(before.image_url, after.image_url);
            assert_eq!(before.usage_id, after.usage_id);
        }
    }
    let at = Instant::now();
    let retired = black_box(published.replace(Published {
        owner: 1,
        catalog: next,
    }))
    .unwrap();
    let replace_owner_ms = elapsed(at);
    assert_eq!(retired.owner, 0);
    assert_eq!(published.as_ref().unwrap().owner, 1);
    let at = Instant::now();
    drop(black_box(retired));
    let retire_shared_old_ms = elapsed(at);
    assert_eq!(Arc::strong_count(&reader), 1);
    assert!(reader.resolve("rocket", None).is_some());
    let at = Instant::now();
    drop(black_box(reader));
    let retire_last_old_ms = elapsed(at);
    assert_eq!(Arc::strong_count(&published.as_ref().unwrap().catalog), 1);
    let at = Instant::now();
    drop(black_box(published.take()));
    let retire_last_replacement_ms = elapsed(at);

    // Separate rejected/unpublished snapshot lifetime: no Catalog owns it.
    let (snapshot, capture_unpublished_ms) = capture(b);
    assert_owned(b, &snapshot);
    let at = Instant::now();
    drop(black_box(snapshot));
    let retire_unpublished_snapshot_ms = elapsed(at);
    json!({"round":round,"warmup":round==0,"catalog_names":catalog_names,"catalog_groups":catalog_groups,
        "capture_initial_ms":capture_initial_ms,"sort_initial_ms":sort_initial_ms,"build_initial_ms":build_initial_ms,
        "capture_replacement_ms":capture_replacement_ms,"sort_replacement_ms":sort_replacement_ms,"build_replacement_ms":build_replacement_ms,
        "replace_owner_ms":replace_owner_ms,"retire_shared_old_ms":retire_shared_old_ms,
        "retire_last_old_ms":retire_last_old_ms,"retire_last_replacement_ms":retire_last_replacement_ms,
        "capture_unpublished_ms":capture_unpublished_ms,"retire_unpublished_snapshot_ms":retire_unpublished_snapshot_ms})
}

fn inventory(rows: &HashMap<String, Emoji>) -> Value {
    let mut rows: Vec<_> = rows.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash = 0xcbf29ce484222325u64;
    let mut name_bytes = 0usize;
    let mut value_bytes = 0usize;
    let mut extra_bytes = 0usize;
    let mut map_key_bytes = 0usize;
    let mut aliases = 0usize;
    let mut min_url = usize::MAX;
    let mut max_url = 0usize;
    for (key, row) in &rows {
        let encoded = serde_json::to_vec(row).unwrap();
        for byte in &encoded {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
        map_key_bytes += key.len();
        name_bytes += row.name.len();
        value_bytes += row.value.len();
        extra_bytes += serde_json::to_vec(&row.extra).unwrap().len();
        if row.value.starts_with("alias:") {
            aliases += 1;
        } else {
            min_url = min_url.min(row.value.len());
            max_url = max_url.max(row.value.len());
        }
    }
    json!({"rows":rows.len(),"aliases":aliases,"urls":rows.len()-aliases,
        "fixture_fnv1a":format!("{hash:016x}"),"name_bytes":name_bytes,"value_bytes":value_bytes,
        "extra_json_bytes":extra_bytes,"map_key_bytes_not_captured":map_key_bytes,
        "url_bytes_min":if max_url==0{0}else{min_url},"url_bytes_max":max_url})
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: emoji_catalog_lifecycle <rounds 1..100> <seed 0..1000000>"
    );
    let rounds: usize = args[1].parse().unwrap();
    let seed: usize = args[2].parse().unwrap();
    assert!((1..=100).contains(&rounds) && seed <= 1_000_000);
    let mut inventories = Vec::new();
    for offset in 0..SIZES.len() {
        let count = SIZES[(offset + seed) % SIZES.len()];
        let at = Instant::now();
        let a = corpus(count, 0);
        let b = corpus(count, 1);
        let setup_ms = elapsed(at);
        let before = inventory(&a);
        let after = inventory(&b);
        let samples: Vec<_> = (0..=rounds).map(|round| sample(&a, &b, round)).collect();
        inventories.push(json!({"count":count,"setup_ms":setup_ms,"before":before,"after":after,"samples":samples}));
    }
    println!(
        "{}",
        json!({"schema":1,"corpus":VERSION,"rounds":rounds,"seed":seed,"retained_warmup_rounds":1,
        "measurement":"actual full Emoji clone, deterministic sort, reviewed Catalog::build and Arc/snapshot drop wall time; includes replacement with old owner reader live; assertions/setup/serialization excluded",
        "excludes":["picker projection","preferences/history","ranking","worker transfer/scheduling","renderer/native frames","production owner admission"],
        "inventories":inventories})
    );
}
