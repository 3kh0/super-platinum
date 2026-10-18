use super::*;
use store::Journal;
#[test]
fn cancellation_before_dispatch_keeps_audio_and_legacy_records_default_to_not_cancelled() {
    let mut r = record();
    let mut legacy = serde_json::to_value(&r.body).unwrap();
    legacy.as_object_mut().unwrap().remove("cancel_requested");
    let body: Body = serde_json::from_value(legacy).unwrap();
    assert!(!body.cancel_requested);
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.request_cancel().unwrap();
    assert!(r.recovered().cancel_before_dispatch().is_err());
    r.cancel_before_dispatch().unwrap();
    assert!(r.pending().is_none());
    assert!(!r.cancel_requested());
    assert!(r.audio().is_some());
    assert_eq!(r.form().unwrap().current_file(), Some("F00000001"));
}
#[test]
fn cancelled_upload_survives_restart_and_cannot_attach() {
    let temp = Temp::new();
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.request_cancel().unwrap();
    r.remember_ticket("F00000002").unwrap();
    r.mark(Stage::Bytes).unwrap();
    r.mark(Stage::Complete).unwrap();
    assert!(r.confirm_upload("F00000002").is_err());
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    j.put(None, &r).unwrap();
    drop(j);
    let j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut restored = j.get(&scope()).unwrap().unwrap().record;
    assert!(restored.cancel_requested());
    assert!(restored.confirm_upload("F00000002").is_err());
    assert_eq!(restored.form().unwrap().current_file(), Some("F00000001"));
    restored.prepare_cleanup(1).unwrap();
    restored.mark(Stage::Deleting).unwrap();
    restored.confirm_cleanup("F00000002").unwrap();
    assert!(!restored.cancel_requested());
    assert!(restored.staged_file().is_none());
    assert!(restored.audio().is_some());
}
#[test]
fn cancellation_without_a_ticket_stays_unresolved_after_restart() {
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.request_cancel().unwrap();
    let mut restored = r.recovered();
    assert!(restored.cancel_requested());
    assert!(restored.prepare_cleanup(1).is_err());
    assert!(restored.begin(Operation::Upload, 1).is_err());
    assert!(restored.discard_local_changes().is_err());
    assert!(restored.pending().is_some());
}
const ACCOUNT: &str = "5dfb49d8-7de1-4a1b-9e7e-97c69cc1c835";
fn scope() -> Scope {
    Scope::new(ACCOUNT, "T1", "U1").unwrap()
}
fn baseline() -> OwnProfileSections {
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: String::new(),
        url: String::new(),
        token: String::new(),
    };
    OwnProfileSections::from_response(&w,&json!({"ok":true,"result":{"data":{"user":{"id":"U1","isSelf":true,"profileSections":[{"sectionId":"H1","type":"HEADER","profileElements":[{"elementId":"pronunciation","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":"Zo-ee","permissions":{"ui":false}},{"elementId":"recording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F00000001"}]}]}}}})).unwrap()
}
fn record() -> Record {
    let mut r = Record::capture(scope(), &RecordingDraft::open(&baseline()).unwrap()).unwrap();
    r.set_audio(Audio::new(b"synthetic audio".to_vec(), "audio/webm", 1700).unwrap())
        .unwrap();
    r
}
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("super-platinum-recording-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("recording.sqlite")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn restart_preserves_audio_header_and_microphone_without_credentials() {
    let temp = Temp::new();
    let mut r = record();
    r.set_device("fixture-microphone".into()).unwrap();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let saved = j.put(None, &r).unwrap();
    drop(j);
    let j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let reopened = j.get(&scope()).unwrap().unwrap();
    assert_eq!(saved.revision, reopened.revision);
    assert_eq!(reopened.record.audio().unwrap().bytes(), b"synthetic audio");
    assert_eq!(reopened.record.device(), "fixture-microphone");
    assert!(
        reopened
            .record
            .form()
            .unwrap()
            .revalidate(&baseline())
            .is_ok()
    );
    let encoded = serde_json::to_string(&r.body).unwrap();
    assert!(!encoded.contains("token"));
    assert!(!encoded.contains("upload_url"));
    assert!(encoded.contains("Zo-ee"));
}
#[test]
fn reopened_dispatched_operation_is_unknown_and_cannot_replay() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.remember_ticket("F00000002").unwrap();
    r.mark(Stage::Bytes).unwrap();
    let id = r.pending().unwrap().id().to_owned();
    j.put(None, &r).unwrap();
    drop(j);
    let j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut restored = j.get(&scope()).unwrap().unwrap().record;
    assert!(restored.pending().unwrap().stage() == Stage::Unknown);
    assert_eq!(restored.pending().unwrap().id(), id);
    assert_eq!(restored.staged_file(), Some("F00000002"));
    assert!(restored.mark(Stage::Bytes).is_err());
    assert!(restored.begin(Operation::Upload, 100).is_err());
}
#[test]
fn wrong_account_and_scope_cannot_read_or_replace_record() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    j.put(None, &record()).unwrap();
    let foreign = Scope::new("f5229c44-edcf-4746-9373-d5003a664034", "T1", "U1").unwrap();
    assert!(j.get(&foreign).is_err());
    assert!(Journal::open(temp.path(), foreign.account()).is_err());
    let other = Scope::new(ACCOUNT, "T2", "U1").unwrap();
    assert!(j.get(&other).unwrap().is_none());
    assert!(Record::capture(other, &RecordingDraft::open(&baseline()).unwrap()).is_err());
}
#[test]
fn stale_writer_cannot_overwrite_new_audio_or_shorten_cooldown() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut r = record();
    let first = j.put(None, &r).unwrap();
    r.defer(400);
    let second = j.put(Some(first.revision), &r).unwrap();
    assert!(j.put(Some(first.revision), &record()).is_err());
    assert!(j.put(Some(second.revision), &record()).is_err());
    assert_eq!(j.get(&scope()).unwrap().unwrap().record.retry_at(), 400);
}
#[test]
fn unresolved_and_staged_custody_cannot_be_discarded() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.remember_ticket("F00000002").unwrap();
    r.mark(Stage::Bytes).unwrap();
    r.mark(Stage::Complete).unwrap();
    let saved = j.put(None, &r).unwrap();
    assert!(j.discard_local(&scope(), saved.revision, 3).is_err());
    r.confirm_upload("F00000002").unwrap();
    let saved = j.put(Some(saved.revision), &r).unwrap();
    assert!(j.discard_local(&scope(), saved.revision, 3).is_err());
    r.begin(Operation::Cleanup, 2).unwrap();
    r.mark(Stage::Deleting).unwrap();
    assert!(r.confirm_cleanup("F00000003").is_err());
    r.confirm_cleanup("F00000002").unwrap();
    let saved = j.put(Some(saved.revision), &r).unwrap();
    j.discard_local(&scope(), saved.revision, 3).unwrap();
    assert!(j.get(&scope()).unwrap().is_none());
}
#[test]
fn original_recording_cannot_be_misclassified_as_staged_upload() {
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    assert!(r.remember_ticket("F00000001").is_err());
    assert!(r.remember_ticket("bad").is_err());
    r.remember_ticket("F00000002").unwrap();
    assert!(r.mark(Stage::Deleting).is_err());
}
#[test]
fn corrupt_oversize_and_mismatched_audio_are_retained_not_silently_pruned() {
    for mode in 0..3 {
        let temp = Temp::new();
        let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
        j.put(None, &record()).unwrap();
        let connection = rusqlite::Connection::open(temp.path()).unwrap();
        match mode {
            0 => {
                connection
                    .execute(
                        "UPDATE recordings SET audio=zeroblob(?1)",
                        [MAX_BYTES as i64 + 1],
                    )
                    .unwrap();
            }
            1 => {
                connection
                    .execute("UPDATE recordings SET audio=NULL", [])
                    .unwrap();
            }
            _ => {
                connection
                    .execute(
                        "UPDATE recordings SET body=?1",
                        ["x".repeat(MAX_HEADER_BYTES + 1)],
                    )
                    .unwrap();
            }
        }
        assert!(j.get(&scope()).is_err());
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM recordings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
}
#[test]
fn audio_bounds_and_header_conflicts_fail_closed() {
    assert!(Audio::new(Vec::new(), "audio/webm", 1).is_err());
    assert!(Audio::new(vec![0], "video/webm", 1).is_err());
    assert!(Audio::new(vec![0], "audio/webm", 10001).is_err());
    let mut r = record();
    r.body.header.section["profileElements"][0]["__typename"] = json!("UnknownFutureElement");
    assert!(r.validate().is_err());
}
#[test]
fn cleanup_only_addresses_owned_unattached_private_ticket() {
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.remember_ticket("F00000002").unwrap();
    r.mark(Stage::Bytes).unwrap();
    r.mark(Stage::Complete).unwrap();
    r.confirm_upload("F00000002").unwrap();
    r.begin(Operation::Cleanup, 2).unwrap();
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: String::new(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let file = json!({"id":"F00000002","user":"U1","subtype":SUBTYPE,"mode":"hosted","is_public":false,"public_url_shared":false,"channels":[],"groups":[],"ims":[],"shares":{}});
    let request = r
        .cleanup_request(&SlackClient::default(), &w, &baseline(), &file)
        .unwrap();
    assert!(!request.retry_safe());
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<Vec<_>>(),
        vec![("file".into(), "F00000002".into())]
    );
    for mode in 0..7 {
        let mut altered = file.clone();
        match mode {
            0 => altered["user"] = json!("U2"),
            1 => altered["id"] = json!("F00000001"),
            2 => altered["is_public"] = json!(true),
            3 => altered["channels"] = json!(["C1"]),
            4 => altered["shares"] = json!({"private":{"C1":[{}]}}),
            5 => altered["subtype"] = json!("slack_audio"),
            _ => altered["is_public"] = Value::Null,
        }
        assert!(
            r.cleanup_request(&SlackClient::default(), &w, &baseline(), &altered)
                .is_err()
        );
    }
    let mut raw = baseline().raw().clone();
    raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][1]["fileId"] =
        json!("F00000002");
    let attached = OwnProfileSections::from_response(&w, &raw).unwrap();
    assert!(
        r.cleanup_request(&SlackClient::default(), &w, &attached, &file)
            .is_err()
    );
}
#[test]
fn discard_recreate_never_reuses_revision_for_a_delayed_writer() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let first = j.put(None, &record()).unwrap();
    j.discard_local(&scope(), first.revision, 3).unwrap();
    drop(j);
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    let mut newer = record();
    newer
        .set_audio(Audio::new(b"new synthetic audio".to_vec(), "audio/webm", 1000).unwrap())
        .unwrap();
    let second = j.put(None, &newer).unwrap();
    assert!(second.revision > first.revision);
    assert!(j.put(Some(first.revision), &first.record).is_err());
    assert_eq!(
        j.get(&scope())
            .unwrap()
            .unwrap()
            .record
            .audio()
            .unwrap()
            .bytes(),
        b"new synthetic audio"
    );
}
#[test]
fn remove_before_save_retains_unused_staged_upload_and_audio() {
    let mut r = record();
    r.begin(Operation::Upload, 1).unwrap();
    r.mark(Stage::Ticket).unwrap();
    r.remember_ticket("F00000002").unwrap();
    r.mark(Stage::Bytes).unwrap();
    r.mark(Stage::Complete).unwrap();
    r.confirm_upload("F00000002").unwrap();
    let mut form = r.form().unwrap();
    form.remove();
    r.set_form(&form).unwrap();
    r.begin(Operation::Save, 2).unwrap();
    r.mark(Stage::Saving).unwrap();
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: String::new(),
        url: String::new(),
        token: String::new(),
    };
    let mut raw = baseline().raw().clone();
    raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][1]["fileId"] =
        Value::Null;
    let fresh = OwnProfileSections::from_response(&w, &raw).unwrap();
    r.confirm_save(&fresh).unwrap();
    assert_eq!(r.staged_file(), Some("F00000002"));
    assert!(r.audio().is_some());
    assert!(!r.form().unwrap().dirty());
    r.begin(Operation::Cleanup, 3).unwrap();
}
#[test]
fn cooldown_survives_before_first_header_and_cannot_be_shortened() {
    let temp = Temp::new();
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    j.defer(&scope(), 500).unwrap();
    j.defer(&scope(), 100).unwrap();
    assert!(j.get(&scope()).unwrap().is_none());
    drop(j);
    let mut j = Journal::open(temp.path(), ACCOUNT).unwrap();
    assert_eq!(j.cooldown(&scope()).unwrap(), 500);
    assert!(j.put(None, &record()).is_err());
    let mut r = record();
    r.defer(500);
    j.put(None, &r).unwrap();
    assert_eq!(j.get(&scope()).unwrap().unwrap().record.retry_at(), 500);
}
