use super::{Catalog, CustomRecord};
use crate::composer_emoji::{insertion, search, standard};
use crate::slack::models::Emoji;
use serde_json::json;

#[path = "frozen_catalog.rs"]
mod frozen_catalog;
#[path = "parity.rs"]
mod parity;

fn record(name: &str, value: &str, generation: u64) -> Emoji {
    Emoji {
        name: name.into(),
        value: value.into(),
        updated: Some(generation),
        extra: [(
            "unknown".into(),
            json!({"nested":[generation,"preserve",{"flag":true}]}),
        )]
        .into(),
    }
}

fn check(input: &[Emoji]) {
    let canonical = serde_json::to_value(input).unwrap();
    let thin: Vec<_> = input.iter().map(CustomRecord::from).collect();
    assert_eq!(serde_json::to_value(input).unwrap(), canonical);
    for version in [standard::Version::V16, standard::Version::V17] {
        let old = frozen_catalog::Catalog::build(version, input.to_vec()).unwrap();
        let compatible = Catalog::build(version, input.to_vec()).unwrap();
        let projected = Catalog::build_records(version, thin.clone()).unwrap();
        parity::assert_same(&old, &compatible);
        parity::assert_same(&old, &projected);
    }
    assert_eq!(serde_json::to_value(input).unwrap(), canonical);
}

#[test]
fn exact_standard_and_custom_alias_override_order_and_tones() {
    check(&[]);
    let rows = [
        record("alias_first", "alias:later", 1),
        record("rocket", "https://fixture.invalid/custom-rocket.png", 2),
        record("later", "https://fixture.invalid/later.png", 3),
        record("chain", "alias:alias_first", 4),
        record("standard_alias", "alias:+1", 5),
        record("rocket_alias", "alias:rocket", 6),
        record("東京_release", "https://fixture.invalid/unicode.png", 7),
    ];
    check(&rows);
    check(&rows.into_iter().rev().collect::<Vec<_>>());
}

#[test]
fn exact_duplicate_dangling_cycle_invalid_name_and_url_behavior() {
    check(&[
        record("same", "https://fixture.invalid/old.png", 1),
        record("alias", "alias:same", 2),
        record("same", "https://fixture.invalid/new.png", 3),
        record("alias", "alias:rocket", 4),
        record("dangling", "alias:no_target", 5),
        record("cycle_a", "alias:cycle_b", 6),
        record("cycle_b", "alias:cycle_a", 7),
        record("self", "alias:self", 8),
        record("", "https://fixture.invalid/empty.png", 9),
        record("bad:name", "https://fixture.invalid/bad.png", 10),
        record("bad\nname", "https://fixture.invalid/control.png", 11),
        record("<bad>", "https://fixture.invalid/bracket.png", 12),
        record("space name", "https://fixture.invalid/space.png", 13),
        record("bad_url", "not a URL", 14),
        record("file_url", "file:///fixture/file.png", 15),
        record("missing_host", "https://", 16),
    ]);
}

#[test]
fn metadata_only_changes_do_not_change_catalog_and_capture_is_owned() {
    let mut canonical = vec![record("custom", "https://fixture.invalid/a.png", 1)];
    let before = serde_json::to_value(&canonical).unwrap();
    let captured: Vec<_> = canonical.iter().map(CustomRecord::from).collect();
    assert_eq!(serde_json::to_value(&canonical).unwrap(), before);
    canonical[0].updated = Some(999);
    canonical[0]
        .extra
        .insert("more".into(), json!({"arbitrary":[1,2,3]}));
    let old = frozen_catalog::Catalog::build(standard::Version::V16, canonical.clone()).unwrap();
    let thin = Catalog::build_records(standard::Version::V16, captured.clone()).unwrap();
    parity::assert_same(&old, &thin);
    canonical[0].name = "renamed".into();
    canonical[0].value = "https://fixture.invalid/b.png".into();
    assert_eq!(captured[0].name, "custom");
    assert_eq!(captured[0].value, "https://fixture.invalid/a.png");
    check(&canonical);
}
