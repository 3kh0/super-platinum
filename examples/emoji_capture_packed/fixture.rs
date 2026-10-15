use super::{Emoji, HashMap, Value, json};
pub(super) fn name(index: usize) -> String {
    format!("workspace_release_train_{index:06}_celebration_custom_asset")
}

pub(super) fn corpus(count: usize, generation: usize) -> HashMap<String, Emoji> {
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

pub(super) fn inventory(rows: &HashMap<String, Emoji>) -> Value {
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
