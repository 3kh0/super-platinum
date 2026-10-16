use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use super_platinum_core::file_sharing::{
    Scope,
    store::{Begin, Journal},
};
use super_platinum_core::slack::api::file_share::FileShareDisposition;

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new(case: Case) -> Self {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let name = format!(
            "sp-share-cap-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let path = base.join(name);
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("journal")).unwrap();
        std::fs::write(path.join("file-share-fixture.json"), serde_json::to_vec(&json!({
            "schema":1,"kind":"file-share-fixture","nonce":ACCOUNT,"fixture":"file-share","case":case
        })).unwrap()).unwrap();
        Self(path)
    }
    fn inputs(&self, case: Case) -> Inputs {
        Inputs {
            fixture_mode: true,
            fixture: "file-share".into(),
            enabled: true,
            case,
            nonce: ACCOUNT.into(),
            database: self.0.join("journal/file-share.sqlite"),
            saved_database: self.0.join("saved-file-recovery.sqlite"),
            ack: self.0.clone(),
        }
    }
    fn capability(&self, case: Case) -> Capability {
        Capability::for_test(self.0.clone(), case, 7, 11).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let Ok(path) = self.0.canonicalize() else {
            return;
        };
        let Ok(base) = std::env::temp_dir().canonicalize() else {
            return;
        };
        if path.parent() == Some(base.as_path())
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("sp-share-cap-"))
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn begin(cap: &Capability) -> (Journal, Receipt) {
    let scope = Scope {
        team: TEAM.into(),
        user: USER.into(),
        file: FILE.into(),
        destination: DESTINATION.into(),
    };
    let note = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Café 日本語","style":{"bold":true}}]}]}),
    ];
    let intent = Intent::new(FILE.into(), DESTINATION.into(), Some(note), None).unwrap();
    let mut journal = Journal::open(cap.storage_path(), ACCOUNT).unwrap();
    let Begin::Started(receipt) = journal
        .begin(
            scope,
            intent,
            "Fixture file".into(),
            "Share fixture".into(),
            10,
        )
        .unwrap()
    else {
        panic!("fresh Begin required")
    };
    (journal, receipt)
}
fn read_pair(cap: &Capability) {
    assert_eq!(cap.source_info(FILE).unwrap()["file"]["id"], FILE);
    assert_eq!(
        cap.destination_info(DESTINATION).unwrap()["channel"]["id"],
        DESTINATION
    );
}

#[test]
fn named_capability_rejects_each_missing_gate_and_marker_mismatch() {
    let temp = Temp::new(Case::Accepted);
    for index in 0..5 {
        let mut input = temp.inputs(Case::Accepted);
        match index {
            0 => input.fixture_mode = false,
            1 => input.enabled = false,
            2 => input.fixture = "message-file-actions".into(),
            3 => input.nonce = "ab".into(),
            _ => input.case = Case::Unconfirmed,
        }
        assert!(Paths::load(&input).is_err());
    }
    assert!(Paths::load(&temp.inputs(Case::Accepted)).is_ok());
    std::fs::write(temp.0.join("file-share-fixture.json"), b"{\"schema\":1,\"kind\":\"file-share-fixture\",\"nonce\":\"89fce7ae-a2c9-4c74-919b-4176535d5d70\",\"fixture\":\"file-share\",\"case\":\"accepted\",\"extra\":true}").unwrap();
    assert!(Paths::load(&temp.inputs(Case::Accepted)).is_err());
}

#[test]
fn paths_reject_escape_missing_marker_and_either_existing_database() {
    let temp = Temp::new(Case::Accepted);
    let mut input = temp.inputs(Case::Accepted);
    input.database = temp.0.join("elsewhere.sqlite");
    assert!(Paths::load(&input).is_err());
    for filename in ["journal/file-share.sqlite", "saved-file-recovery.sqlite"] {
        std::fs::write(temp.0.join(filename), b"existing").unwrap();
        assert!(Paths::load(&temp.inputs(Case::Accepted)).is_err());
        std::fs::remove_file(temp.0.join(filename)).unwrap();
    }
    std::fs::remove_file(temp.0.join("file-share-fixture.json")).unwrap();
    assert!(Paths::load(&temp.inputs(Case::Accepted)).is_err());
}

#[test]
fn one_actual_builder_request_holds_response_and_keeps_unicode_note() {
    let temp = Temp::new(Case::Accepted);
    let cap = temp.capability(Case::Accepted);
    let (mut journal, receipt) = begin(&cap);
    read_pair(&cap);
    assert!(cap.record_begin(&receipt).unwrap().is_none());
    let request = cap.prepare(&receipt.intent).unwrap();
    assert!(
        request
            .url
            .starts_with(&format!("{ORIGIN}/api/files.share?"))
    );
    let mut receive = cap.record_prepared(&receipt, request).unwrap();
    assert!(matches!(
        receive.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(cap.release("other-attempt").is_err());
    assert!(
        cap.record_prepared(&receipt, cap.prepare(&receipt.intent).unwrap())
            .is_err()
    );
    cap.release(&receipt.attempt_id).unwrap();
    let result = receive.try_recv().unwrap();
    let saved = journal
        .finish_result(&receipt.scope, &receipt.attempt_id, result, 20)
        .unwrap()
        .unwrap();
    assert!(matches!(
        saved.state(),
        State::Server(FileShareDisposition::Accepted { .. })
    ));
    assert!(saved.intent.note() == receipt.intent.note());
    let evidence = cap.evidence().unwrap();
    assert_eq!(evidence.dispatch_count, 1);
    let fields = evidence.fields.unwrap();
    assert_eq!(fields[0], ("token".into(), String::new()));
    assert_eq!(
        serde_json::from_str::<Value>(&fields[3].1).unwrap(),
        serde_json::to_value(receipt.intent.note()).unwrap()
    );
    assert!(cap.release(&receipt.attempt_id).is_err());
    assert!(cap.source_info(FILE).is_err());
    assert!(cap.same_identity(&cap.clone()));
}

#[test]
fn dispatch_rejects_no_begin_wrong_read_order_and_extra_flag() {
    let temp = Temp::new(Case::Accepted);
    let cap = temp.capability(Case::Accepted);
    let (_journal, receipt) = begin(&cap);
    let request = cap.prepare(&receipt.intent).unwrap();
    assert!(cap.record_prepared(&receipt, request.clone()).is_err());
    cap.record_begin(&receipt).unwrap();
    cap.destination_info(DESTINATION).unwrap();
    cap.source_info(FILE).unwrap();
    assert!(cap.record_prepared(&receipt, request).is_err());
    let temp2 = Temp::new(Case::Accepted);
    let cap2 = temp2.capability(Case::Accepted);
    let (_journal2, receipt2) = begin(&cap2);
    cap2.record_begin(&receipt2).unwrap();
    read_pair(&cap2);
    let mut request = cap2.prepare(&receipt2.intent).unwrap();
    if let RequestBody::Form(fields) = &mut request.body {
        fields.push(("allow_external".into(), "true".into()));
    }
    assert!(cap2.record_prepared(&receipt2, request).is_err());
    assert_eq!(cap2.evidence().unwrap().dispatch_count, 0);
}

#[test]
fn response_dispositions_are_real_journal_results_and_reopen_has_no_dispatch() {
    for case in [Case::DlpWarning, Case::Processing, Case::Unconfirmed] {
        let temp = Temp::new(case);
        let cap = temp.capability(case);
        let (mut journal, receipt) = begin(&cap);
        read_pair(&cap);
        cap.record_begin(&receipt).unwrap();
        let mut receive = cap
            .record_prepared(&receipt, cap.prepare(&receipt.intent).unwrap())
            .unwrap();
        cap.release(&receipt.attempt_id).unwrap();
        let saved = journal
            .finish_result(
                &receipt.scope,
                &receipt.attempt_id,
                receive.try_recv().unwrap(),
                20,
            )
            .unwrap()
            .unwrap();
        match case {
            Case::DlpWarning => assert!(matches!(
                saved.state(),
                State::Server(FileShareDisposition::DlpWarning { .. })
            )),
            Case::Processing => assert!(matches!(
                saved.state(),
                State::Server(FileShareDisposition::DlpProcessing)
            )),
            Case::Unconfirmed => assert_eq!(saved.state(), State::Unconfirmed),
            _ => unreachable!(),
        }
        drop(journal);
        let mut reopened = Journal::open(cap.storage_path(), ACCOUNT).unwrap();
        let held = reopened.get(&receipt.scope).unwrap().unwrap();
        assert!(held == saved);
        assert!(matches!(
            reopened
                .begin(
                    receipt.scope.clone(),
                    receipt.intent.clone(),
                    "Another".into(),
                    "Another".into(),
                    30
                )
                .unwrap(),
            Begin::Blocked(_)
        ));
        assert!(
            cap.record_prepared(&held, cap.prepare(&held.intent).unwrap())
                .is_err()
        );
        assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
    }
}

#[test]
fn stale_source_checkpoint_is_one_shot_and_never_dispatches() {
    let temp = Temp::new(Case::SourceStale);
    let cap = temp.capability(Case::SourceStale);
    let (_journal, receipt) = begin(&cap);
    read_pair(&cap);
    let mut receive = cap.record_begin(&receipt).unwrap().unwrap();
    assert!(matches!(
        receive.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(cap.release_begin("wrong").is_err());
    assert!(
        cap.record_prepared(&receipt, cap.prepare(&receipt.intent).unwrap())
            .is_err()
    );
    cap.release_begin(&receipt.attempt_id).unwrap();
    receive.try_recv().unwrap();
    assert!(cap.release_begin(&receipt.attempt_id).is_err());
    assert!(cap.record_begin(&receipt).is_err());
    assert_eq!(cap.evidence().unwrap().dispatch_count, 0);
}

#[test]
fn independent_real_builder_nonces_are_valid_without_full_url_equality() {
    let temp = Temp::new(Case::Accepted);
    let cap = temp.capability(Case::Accepted);
    let (_journal, receipt) = begin(&cap);
    let first = cap.prepare(&receipt.intent).unwrap();
    let second = cap.prepare(&receipt.intent).unwrap();
    assert_ne!(first.url, second.url);
    assert!(request::matches(&first, &second));
    assert!(request::matches(&second, &first));
    read_pair(&cap);
    cap.record_begin(&receipt).unwrap();
    let _response = cap.record_prepared(&receipt, first).unwrap();
    assert_eq!(cap.evidence().unwrap().dispatch_count, 1);
}

#[test]
fn request_oracle_rejects_query_shape_and_all_stable_request_drift() {
    let temp = Temp::new(Case::Accepted);
    let cap = temp.capability(Case::Accepted);
    let (_journal, receipt) = begin(&cap);
    let expected = cap.prepare(&receipt.intent).unwrap();
    let actual = cap.prepare(&receipt.intent).unwrap();
    let (base, query) = actual.url.split_once('?').unwrap();
    let pairs: Vec<_> = query.split('&').map(str::to_owned).collect();
    let first_nonce = pairs[0].split_once('=').unwrap().1;
    let mut invalid = Vec::new();
    for bad in [
        format!("A{}", &first_nonce[1..]),
        format!("{}", &first_nonce[..31]),
        format!("{}1{}", &first_nonce[..12], &first_nonce[13..]),
        format!("{}7{}", &first_nonce[..16], &first_nonce[17..]),
        format!("%{}", first_nonce),
        "00000000-0000-4000-8000-000000000000".into(),
    ] {
        let mut altered = pairs.clone();
        altered[0] = format!("_x_id={bad}");
        invalid.push(format!("{base}?{}", altered.join("&")));
    }
    let mut duplicated_nonce = pairs.clone();
    duplicated_nonce[1] = format!("_x_csid={first_nonce}");
    invalid.push(format!("{base}?{}", duplicated_nonce.join("&")));
    let mut reordered = pairs.clone();
    reordered.swap(0, 1);
    invalid.push(format!("{base}?{}", reordered.join("&")));
    let mut duplicate_key = pairs.clone();
    duplicate_key[1] = pairs[0].clone();
    invalid.push(format!("{base}?{}", duplicate_key.join("&")));
    invalid.extend([
        format!("{base}?{}", pairs[1..].join("&")),
        format!("{}&_x_extra=true", actual.url),
        format!("{}#fragment", actual.url),
        actual.url.replace("_x_num_retries=0", "_x_num_retries=1"),
        actual
            .url
            .replace("_x_version_ts=capture-required", "_x_version_ts=other"),
        actual
            .url
            .replace("_x_foreground=true", "_x_foreground=false"),
        actual
            .url
            .replace("slack_route=default", "slack_route=%64efault"),
        actual
            .url
            .replace("/api/files.share?", "/api/files.share/other?"),
        actual.url.replace(ORIGIN, "https://different.invalid"),
        actual
            .url
            .replace(ORIGIN, "https://file-share-fixture.invalid:443"),
        actual.url.replace("_x_id=", "%5Fx_id="),
    ]);
    for url in invalid {
        let mut mutated = actual.clone();
        mutated.url = url;
        assert!(!request::matches(&mutated, &expected));
        assert!(cap.record_prepared(&receipt, mutated).is_err());
    }
    let mut method = actual.clone();
    method.method = "GET";
    assert!(!request::matches(&method, &expected));
    let mut headers = actual.clone();
    headers
        .headers
        .push(("fixture-extra".into(), "true".into()));
    assert!(!request::matches(&headers, &expected));
    let mut body = actual.clone();
    let RequestBody::Form(fields) = &mut body.body else {
        panic!("form required")
    };
    fields[0].1 = "unexpected-token".into();
    assert!(!request::matches(&body, &expected));
    assert_eq!(cap.evidence().unwrap().dispatch_count, 0);
}

#[test]
fn fresh_main_files_do_not_allow_existing_sqlite_sidecars() {
    let temp = Temp::new(Case::Accepted);
    for database in ["journal/file-share.sqlite", "saved-file-recovery.sqlite"] {
        for suffix in ["-wal", "-shm", "-journal"] {
            let sidecar = temp.0.join(format!("{database}{suffix}"));
            std::fs::write(&sidecar, b"prior sidecar").unwrap();
            assert!(Paths::load(&temp.inputs(Case::Accepted)).is_err());
            std::fs::remove_file(sidecar).unwrap();
        }
    }
    assert!(Paths::load(&temp.inputs(Case::Accepted)).is_ok());
}

#[test]
fn seeded_nonowner_hosted_file_has_real_action_permalink_and_share_policy() {
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(source_file()).unwrap();
    assert_eq!(
        super_platinum_core::file_actions::permalink(&file).as_deref(),
        Some("https://fixture.slack.com/files/U00000002/F00000001")
    );
    assert_eq!(
        super_platinum_core::slack::file_share_policy::source(&file),
        super_platinum_core::slack::file_share_policy::Eligibility::Allowed
    );
    assert_ne!(file.extra.get("user").and_then(Value::as_str), Some(USER));
    assert_eq!(
        source_file()["url_private"],
        "https://file-share-fixture.invalid/source"
    );
}

#[test]
fn pressure_alias_gate_requires_exact_name_enabled_fresh_accepted_capability() {
    let temp = Temp::new(Case::Accepted);
    for alias in ["file-share-pressure", "file-share-pressure-narrow"] {
        let mut inputs = temp.inputs(Case::Accepted);
        inputs.fixture = alias.into();
        assert!(inputs.gate().is_ok());
        assert!(
            Paths::load(&inputs).is_err(),
            "marker alias cannot fall back"
        );
        std::fs::write(temp.0.join("file-share-fixture.json"),serde_json::to_vec(&json!({"schema":1,"kind":"file-share-fixture","nonce":ACCOUNT,"fixture":alias,"case":"accepted"})).unwrap()).unwrap();
        assert!(Paths::load(&inputs).is_ok(), "fresh exact marker accepted");
        inputs.case = Case::StorageFailure;
        assert_eq!(inputs.gate(), Err(Rejected::Gate));
        inputs.case = Case::Accepted;
        inputs.enabled = false;
        assert_eq!(inputs.gate(), Err(Rejected::Gate));
    }
    let mut inputs = temp.inputs(Case::Accepted);
    inputs.fixture = "file-share-pressure-other".into();
    assert_eq!(inputs.gate(), Err(Rejected::Gate));
}
