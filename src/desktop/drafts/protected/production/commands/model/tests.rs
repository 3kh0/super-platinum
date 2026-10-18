use super::*;
use serde_json::json;
use super_platinum_core::slash_commands::Owner;

fn catalog(thread: bool) -> Catalog {
    Catalog::decode(Owner {
        account: "00000000-0000-4000-8000-000000000001".into(),
        team:"T1".into(), user:"U1".into(), channel:"C1".into(),
        thread:thread.then(|| "1.000001".into()),
    }, json!({"ok":true,"app_actions":[],"commands":[
        {"type":"app","name":"/example","app":"A1","app_name":"First","teams":["T1"],"is_runnable_from_thread":true},
        {"type":"app","name":"/example","app":"A2","app_name":"Second","teams":["T1"]},
        {"type":"app","name":"/other","app":"A3","teams":["T1"]},
        {"type":"app","name":"/example","app":"A4","teams":["T2"]},
        {"type":"app","name":"/example","app":"A5","teams":["T1"],"autocomplete":false},
        {"type":"builtin","name":"/example"}
    ]})).unwrap()
}
#[test]
fn actual_catalog_keeps_ambiguous_apps_explicit_and_rejects_unlisted_choice() {
    let catalog = catalog(false);
    let rows = choices(&catalog, "/EXAMPLE actual arguments");
    assert_eq!(
        rows,
        vec![
            (0, "/example \u{b7} First".into()),
            (1, "/example \u{b7} Second".into())
        ]
    );
    for index in [2, 3, 4, 5, usize::MAX] {
        assert!(choice(&catalog, "/example", index).is_none());
    }
    assert!(choices(&catalog, "/exam").is_empty());
    assert!(choices(&catalog, "ordinary /example").is_empty());
    assert_eq!(catalog.unsupported(), 1);
}
#[test]
fn actual_thread_policy_and_catalog_generation_remain_authoritative() {
    let first = catalog(true);
    assert_eq!(choices(&first, "/example x").len(), 1);
    assert!(choice(&first, "/example", 1).is_none());
    let selected = choice(&first, "/example", 0).unwrap();
    let replaced = catalog(true);
    assert!(selected.admit(&replaced, replaced.owner()).is_err());
    assert!(selected.admit(&first, first.owner()).is_ok());
    assert!(choice(&first, "/other", 0).is_none());
}
#[test]
fn later_short_rate_limit_cannot_erase_longer_retained_deadline() {
    let now = Instant::now();
    let key = ("account".into(), "T1".into());
    let mut values = BTreeMap::new();
    let long = remember_cooldown(
        &mut values,
        key.clone(),
        &NetworkError::RateLimited {
            retry_after_secs: Some(120),
        },
        now,
    )
    .unwrap();
    let short = remember_cooldown(
        &mut values,
        key.clone(),
        &NetworkError::HttpStatus {
            status: 429,
            retry_after_secs: Some(2),
        },
        now,
    )
    .unwrap();
    assert_eq!(long, short);
    assert_eq!(values.get(&key), Some(&Some(long)));
    assert_eq!(
        remember_cooldown(
            &mut values,
            key.clone(),
            &NetworkError::Offline("test".into()),
            now
        ),
        None
    );
    assert_eq!(values.get(&key), Some(&Some(long)));
}
#[test]
fn overflow_is_conservatively_retained_and_accounts_do_not_share_deadlines() {
    let now = Instant::now();
    let key = ("account".into(), "T1".into());
    let mut values = BTreeMap::new();
    values.insert(key.clone(), None);
    assert_eq!(
        remember_cooldown(
            &mut values,
            key.clone(),
            &NetworkError::RateLimited {
                retry_after_secs: Some(1)
            },
            now
        ),
        None
    );
    assert_eq!(values.get(&key), Some(&None));
    let other = ("other".into(), "T1".into());
    assert!(
        remember_cooldown(
            &mut values,
            other.clone(),
            &NetworkError::RateLimited {
                retry_after_secs: Some(4)
            },
            now
        )
        .is_some()
    );
    assert_eq!(values.len(), 2);
    assert_eq!(values.get(&key), Some(&None));
}
