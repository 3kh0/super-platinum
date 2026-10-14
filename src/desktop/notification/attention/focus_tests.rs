use super::*;
use serde_json::json;

fn assertions() -> Value {
    json!({"data":[{"storeAssertionRecords":[{
        "assertionDetails":{"assertionDetailsModeIdentifier":"work"},
        "expirationDate":0
    }]}]})
}
fn schedule(start: u16, end: u16) -> Value {
    json!({"data":[{"modeConfigurations":{"NONE":{"triggers":{"triggers":[{
        "enabledSetting":2,"timePeriodStartTimeHour":start,
        "timePeriodStartTimeMinute":30,"timePeriodEndTimeHour":end,
        "timePeriodEndTimeMinute":59,"days":[]
    }]}}}}]})
}
fn secure() -> Value {
    json!({"data":[{"secureModeConfigurations":{"work":{"secureConfiguration":{
        "allowedApplications":{"com.example.test":{}}
    }}}}]})
}

#[test]
fn assertions_use_first_mode_without_invented_expiry() {
    let mut value = assertions();
    value["data"][0]["storeAssertionRecords"]
        .as_array_mut()
        .unwrap()
        .push(json!({"assertionDetails":{"assertionDetailsModeIdentifier":"other"}}));
    assert_eq!(assertion(&value), Ok((true, "work")));
    for records in [json!(null), json!([])] {
        assert_eq!(
            assertion(&json!({"data":[{"storeAssertionRecords":records}]})),
            Ok((false, "NONE"))
        );
    }
    assert_eq!(assertion(&json!({"data":[{}]})), Ok((false, "NONE")));
}

#[test]
fn malformed_required_shapes_are_unknown() {
    for value in [
        json!(null),
        json!({}),
        json!({"data":[]}),
        json!({"data":[null]}),
        json!({"data":[{"storeAssertionRecords":{}}]}),
        json!({"data":[{"storeAssertionRecords":[{}]}]}),
    ] {
        assert!(assertion(&value).is_err());
    }
    assert!(
        allowed(
            &json!({"data":[{"secureModeConfigurations":[]}]}),
            "work",
            "app"
        )
        .is_err()
    );
}

#[test]
fn source_schedule_boundaries_overnight_and_ignored_fields() {
    let value = schedule(8, 17);
    assert_eq!(scheduled(&value, "NONE", 8 * 60 + 29), Ok(false));
    assert_eq!(scheduled(&value, "NONE", 8 * 60 + 30), Ok(true));
    assert_eq!(scheduled(&value, "NONE", 17 * 60), Ok(false));
    let night = schedule(22, 7);
    for now in [22 * 60 + 30, 23 * 60, 0, 7 * 60 - 1] {
        assert_eq!(scheduled(&night, "NONE", now), Ok(true));
    }
    assert_eq!(scheduled(&night, "NONE", 7 * 60), Ok(false));
    assert_eq!(scheduled(&night, "NONE", 22 * 60 + 29), Ok(false));
    let mut equal = schedule(8, 8);
    equal["data"][0]["modeConfigurations"]["NONE"]["triggers"]["triggers"][0]["timePeriodStartTimeMinute"] =
        json!(0);
    assert_eq!(scheduled(&equal, "NONE", 8 * 60), Ok(false));
    assert!(scheduled(&night, "NONE", 1440).is_err());
}

#[test]
fn allowlist_retains_source_child_match_but_never_empty_identity() {
    let value = secure();
    assert_eq!(allowed(&value, "work", "com.example.test"), Ok(true));
    assert_eq!(allowed(&value, "work", "com.example.test.helper"), Ok(true));
    assert_eq!(allowed(&value, "work", "com.example.other"), Ok(false));
    assert_eq!(allowed(&value, "work", ""), Ok(false));
    assert_eq!(allowed(&value, "other", "com.example.test"), Ok(false));
}

#[test]
fn isolated_files_cover_missing_active_clear_and_bounded_reads() {
    let home = std::env::temp_dir().join(format!(
        "sp-focus-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let directory = home.join("Library/DoNotDisturb/DB");
    std::fs::create_dir_all(&directory).unwrap();
    let write = |name: &str, value: Value| {
        std::fs::write(directory.join(name), serde_json::to_vec(&value).unwrap()).unwrap()
    };
    assert_eq!(read(&home, None, 0), Status::Unknown);
    write("Assertions.json", assertions());
    assert_eq!(read(&home, None, 0), Status::Quiet);
    assert_eq!(read(&home, Some(""), 0), Status::Quiet);
    assert_eq!(read(&home, Some("com.example.test"), 0), Status::Unknown);
    write("ModeConfigurationsSecure.json", secure());
    assert_eq!(read(&home, Some("com.example.test"), 0), Status::Available);
    assert_eq!(read(&home, Some("com.example.other"), 0), Status::Quiet);
    write("Assertions.json", json!({"data":[{}]}));
    assert_eq!(read(&home, None, 0), Status::Unknown);
    write("ModeConfigurations.json", schedule(22, 7));
    assert_eq!(read(&home, None, 0), Status::Quiet);
    assert_eq!(read(&home, None, 12 * 60), Status::Available);
    std::fs::write(
        directory.join("Assertions.json"),
        vec![b' '; (MAX_BYTES + 1) as usize],
    )
    .unwrap();
    assert_eq!(read(&home, None, 0), Status::Unknown);
    std::fs::remove_dir_all(home).unwrap();
}
