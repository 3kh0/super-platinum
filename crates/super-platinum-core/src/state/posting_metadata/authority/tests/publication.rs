use super::*;
use crate::slack::models::Channel;
fn next() -> (Value, Channel) {
    let raw = json!({"id":"C1","name":"actual","context_team_id":"T1","is_read_only":true,"is_thread_only":false,"opaque":"kept"});
    let typed = serde_json::from_value(raw.clone()).unwrap();
    (raw, typed)
}
#[test]
fn paired_actual_raw_survives_only_its_predicted_typed_publication() {
    let (mut core, a) = acquired();
    let (raw, typed) = next();
    let old = a.raw_channel(&core, "C1").unwrap();
    let read = a.begin_channel_read(&mut core, "C1").unwrap();
    let publication = read
        .prepare_publication(&core, raw.clone(), &typed)
        .unwrap();
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("C1".into(), typed);
    assert!(!old.current(&core));
    let fresh = publication.apply(&mut core).unwrap();
    assert!(fresh.current(&core));
    assert_eq!(fresh.value(), &raw);
    let flags = a
        .metadata(&core)
        .unwrap()
        .effective(a.configuration(), fresh.value())
        .unwrap();
    assert!(flags.read_only);
    assert!(!flags.thread_only);
}
#[test]
fn paired_posting_refresh_keeps_free_and_standard_feature_access_unknown() {
    use crate::block_actions::posting_policy;

    for plan in [None, Some("std")] {
        let mut core = core();
        core.session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .enterprise_id = None;
        let mut boot = raw_boot();
        boot["team"] = json!({"id":"T1"});
        if let Some(plan) = plan {
            boot["team"]["plan"] = json!(plan);
        }
        boot["self"] = json!({"id":"U1","team_id":"T1"});
        boot["workspaces"] = json!([{"id":"T1"}]);
        boot["channels"] = json!([{"id":"C1","name":"ordinary","is_channel":true,
            "is_general":false,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false}]);
        assert!(begin(&mut core).apply(&mut core, boot, true).unwrap());
        let a = Authority::capture(&core, "T1").unwrap();
        let old = a.raw_channel(&core, "C1").unwrap();
        assert_eq!(
            a.metadata(&core)
                .unwrap()
                .effective(a.configuration(), old.value())
                .unwrap(),
            posting_policy::Flags {
                read_only: false,
                thread_only: false
            }
        );
        let mut raw = old.value().clone();
        raw["is_read_only"] = json!(true);
        let typed = serde_json::from_value(raw.clone()).unwrap();
        let read = a.begin_channel_read(&mut core, "C1").unwrap();
        let publication = read
            .prepare_publication(&core, raw.clone(), &typed)
            .unwrap();
        core.workspaces
            .get_mut("T1")
            .unwrap()
            .channels
            .insert("C1".into(), typed);
        let fresh = publication.apply(&mut core).unwrap();
        assert!(!old.current(&core));
        assert!(fresh.current(&core));
        assert_eq!(fresh.value(), &raw);
        assert_eq!(
            core.workspaces["T1"].channels["C1"].extra["is_read_only"],
            true
        );
        assert_eq!(
            a.metadata(&core)
                .unwrap()
                .effective(a.configuration(), fresh.value()),
            Err(Unknown::Policy(posting_policy::Unknown::FeatureAccess))
        );
    }
}
#[test]
fn paired_publication_rejects_account_reservation_and_foreign_typed_changes() {
    for kind in 0..3 {
        let (mut core, a) = acquired();
        let (raw, mut typed) = next();
        let read = a.begin_channel_read(&mut core, "C1").unwrap();
        let publication = read.prepare_publication(&core, raw, &typed).unwrap();
        match kind {
            0 => {
                core.account_epoch += 1;
            }
            1 => {
                a.begin_channel_read(&mut core, "C1").unwrap();
            }
            _ => {
                typed.extra.insert("newer".into(), json!(true));
            }
        }
        core.workspaces
            .get_mut("T1")
            .unwrap()
            .channels
            .insert("C1".into(), typed);
        assert!(publication.apply(&mut core).is_err());
    }
}
#[test]
fn paired_publication_cannot_be_prepared_from_stale_or_wrong_channel() {
    let (mut core, a) = acquired();
    let (raw, mut typed) = next();
    let read = a.begin_channel_read(&mut core, "C1").unwrap();
    a.begin_channel_read(&mut core, "C1").unwrap();
    assert!(
        read.prepare_publication(&core, raw.clone(), &typed)
            .is_err()
    );
    let read = a.begin_channel_read(&mut core, "C1").unwrap();
    typed.id = "C2".into();
    assert!(read.prepare_publication(&core, raw, &typed).is_err());
}
#[test]
fn accepted_channel_event_retires_prepared_publication() {
    let (mut core, a) = acquired();
    let (raw, typed) = next();
    let read = a.begin_channel_read(&mut core, "C1").unwrap();
    let publication = read.prepare_publication(&core, raw, &typed).unwrap();
    let generation = core.workspaces["T1"].rt_generation;
    observe_event(
        &mut core,
        "T1",
        generation,
        &event(json!({"type":"channel_updated","channel":"C1","updates":{"is_read_only":false}})),
    );
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("C1".into(), typed);
    assert!(publication.apply(&mut core).is_err());
}
