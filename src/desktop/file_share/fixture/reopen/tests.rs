use super::*;

fn values() -> (Value, Value, Value) {
    let run = "baf2aac9-044d-4dba-84bd-df1659ffb897";
    let capture = "5bba8218-c7cf-4f2d-bb42-64d35ad124ae";
    let nonce = "b5fc1e81-eab7-4e6b-aab6-a6b1d1e703a6";
    let hash = "a".repeat(64);
    (
        json!({"schema":1,"kind":"file-share-draft-reopen","run":run,"nonce":nonce,"variant":"exact","input":"text",
        "handoff_sha256":hash,"source_sha256":hash,"executable_sha256":hash,"build_sha256":hash}),
        json!({"schema":1,"kind":"file-share-draft-handoff","run":run,"capture_nonce":capture,"variant":"exact","input":"text",
        "account":ACCOUNT,"team":TEAM,"user":USER,"file":FILE,"source_sha256":hash,"executable_sha256":hash,"build_sha256":hash,
        "database":{"size":4096,"sha256":hash},"draft":{"size":512,"sha256":hash},"audit":{"size":512,"sha256":hash},
        "schema_sha256":include_str!("schema.sha256").trim(),"user_version":2,"counts":{"identity":1,"drafts":1,"receipts":0,"associations":0,"cooldowns":0}}),
        json!({"schema":1,"kind":"file-share-draft-exit","run":run,"nonce":capture,"pid":std::process::id()+1,"creation_time":1,
        "exited":true,"terminal_ok":true,"pointer_unchanged":true,"cleanup_errors":[],"jobs":0,"scopes":0,"running":0,
        "api_reads":0,"begins":0,"dispatches":0}),
    )
}
fn validate(marker: Value, handoff: Value, audit: Value) -> bool {
    match (
        serde_json::from_value(marker),
        serde_json::from_value(handoff),
        serde_json::from_value(audit),
    ) {
        (Ok(m), Ok(h), Ok(a)) => model::valid(&m, &h, &a),
        _ => false,
    }
}
#[test]
fn reopen_contract_binds_owner_source_build_phase_and_actual_exit_shape() {
    let (m, h, a) = values();
    assert!(validate(m.clone(), h.clone(), a.clone()));
    for (field, value) in [
        ("account", json!("wrong")),
        ("team", json!("TOTHER")),
        ("file", json!("FOTHER")),
        ("variant", json!("missing")),
        ("input", json!("destination-only")),
        ("source_sha256", json!("b".repeat(64))),
        ("executable_sha256", json!("b".repeat(64))),
        ("user_version", json!(1)),
        (
            "counts",
            json!({"identity":1,"drafts":1,"receipts":1,"associations":0,"cooldowns":0}),
        ),
    ] {
        let mut wrong = h.clone();
        wrong[field] = value;
        assert!(!validate(m.clone(), wrong, a.clone()));
    }
    for (field, value) in [
        ("exited", json!(false)),
        ("terminal_ok", json!(1)),
        ("cleanup_errors", json!(["failure"])),
        ("running", json!(1)),
        ("api_reads", json!(1)),
        ("begins", json!(1)),
        ("dispatches", json!(1)),
        ("pid", json!(std::process::id())),
        ("creation_time", json!(0)),
    ] {
        let mut wrong = a.clone();
        wrong[field] = value;
        assert!(!validate(m.clone(), h.clone(), wrong));
    }
}
#[test]
fn typed_marker_rejects_unknown_duplicate_nonce_and_boolean_numbers() {
    let (m, h, a) = values();
    for (field, value) in [
        ("extra", json!(false)),
        ("schema", json!(true)),
        ("nonce", m["run"].clone()),
        ("nonce", json!("B5FC1E81-EAB7-4E6B-AAB6-A6B1D1E703A6")),
    ] {
        let mut wrong = m.clone();
        wrong[field] = value;
        assert!(!validate(wrong, h.clone(), a.clone()));
    }
    let text = serde_json::to_string(&m).unwrap();
    let duplicate = text.replacen('{', "{\"schema\":1,", 1);
    assert!(serde_json::from_str::<Marker>(&duplicate).is_err());
}
#[cfg(windows)]
#[test]
fn only_local_absolute_disk_paths_are_accepted() {
    for value in [r"C:\owned\pair", r"\\?\C:\owned\pair"] {
        assert!(super::local_absolute(Path::new(value)));
    }
    for value in [
        r"\\server\share\pair",
        r"\\?\UNC\server\share\pair",
        r"C:relative",
        r"C:\owned\..\other",
    ] {
        assert!(!super::local_absolute(Path::new(value)));
    }
}
#[cfg(windows)]
#[test]
fn cng_sha256_matches_known_bytes_and_enforces_limit() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "sp-restart-hash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(b"abc").unwrap();
    drop(file);
    let result = hash::file(&path, 3);
    let limited = hash::file(&path, 2);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        result.unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(limited.is_err());
}
