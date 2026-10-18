use super::*;
use serde_json::json;
const OWNER: &str = "4b02f26e-393b-4fd1-a7da-5d11f76296ae";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("sp-direct-open-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn open(&self) -> Journal {
        Journal::open(self.0.join("private.sqlite"), OWNER).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn draft(j: &mut Journal) -> drafts::Draft {
    let scope = drafts::DraftScope {
        team: "T00000001".into(),
        user: "U00000001".into(),
        file: "F00000001".into(),
    };
    let content = drafts::Content {
        source: json!({"id":"F00000001","mode":"hosted","is_external":false,"future":{"keep":42}}),
        title: "Original file".into(),
        note: vec![],
        destination: None,
    };
    let drafts::Claim::Claimed(d) = j.claim_draft(scope, None, content).unwrap() else {
        panic!("claim")
    };
    let mut content = d.content.clone();
    content.note = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original 日本語 é","style":{"future":7}}]}]}),
    ];
    content.destination = Some(drafts::Destination {
        id: "person:U00000002".into(),
        name: "Person".into(),
        private: true,
    });
    let drafts::Mutation::Applied(d) = j.save_draft(&d, content).unwrap() else {
        panic!("save")
    };
    d
}
fn peer() -> Value {
    json!({"ok":true,"results":[{"id":"U00000002","deleted":false,"is_bot":false,"future":{"keep":9}}],"failed_ids":[]})
}
fn begin(j: &mut Journal, d: &drafts::Draft) -> Record {
    let Begin::Created(r) = j.begin_direct_open(d, peer(), 10).unwrap() else {
        panic!("created")
    };
    r
}
fn opened() -> Value {
    json!({"ok":true,"channel":{"id":"D00000001"},"future":{"keep":10}})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":"D00000001","user":"U00000002","is_im":true,"is_ext_shared":false,"is_open":true,"future":{"keep":11}}})
}
#[test]
fn restart_preserves_unknown_and_complete_original_without_new_open_authority() {
    let t = Temp::new();
    let mut j = t.open();
    let d = draft(&mut j);
    let r = begin(&mut j, &d);
    drop(j);
    let mut j = t.open();
    let listed = j.direct_opens(&d.scope.team, &d.scope.user).unwrap();
    assert!(listed.as_slice() == [r.clone()]);
    assert!(listed[0].draft() == &d);
    assert!(listed[0].returned().is_none());
    let Begin::Existing(existing) = j.begin_direct_open(&d, peer(), 20).unwrap() else {
        panic!("must not reopen")
    };
    assert!(existing == r);
    assert!(
        j.get(&Scope {
            team: d.scope.team.clone(),
            user: d.scope.user.clone(),
            file: d.scope.file.clone(),
            destination: "D00000001".into()
        })
        .unwrap()
        .is_none()
    );
}
#[test]
fn actual_storage_failure_retries_same_returned_body_after_readonly_resolution() {
    let t = Temp::new();
    let mut j = t.open();
    let d = draft(&mut j);
    let r = begin(&mut j, &d);
    j.conn.execute_batch("CREATE TEMP TRIGGER direct_storage_fault BEFORE UPDATE ON file_share_direct_opens BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END").unwrap();
    assert!(j.direct_open_returned(&r, opened()).is_err());
    assert!(j.direct_opens(&d.scope.team, &d.scope.user).unwrap()[0] == r);
    j.conn
        .execute_batch("DROP TRIGGER direct_storage_fault")
        .unwrap();
    let resolved = j.resolve_direct_open(&r, info()).unwrap();
    assert!(resolved.resolved() == Some(&info()));
    let saved = j.direct_open_returned(&r, opened()).unwrap();
    assert!(saved.returned() == Some(&opened()));
    assert!(saved.resolved() == Some(&info()));
    assert!(j.direct_open_returned(&r, opened()).unwrap() == saved);
    assert!(
        j.direct_open_returned(&r, json!({"ok":true,"channel":{"id":"D00000002"}}))
            .is_err()
    );
    drop(j);
    let j = t.open();
    assert!(j.direct_opens(&d.scope.team, &d.scope.user).unwrap()[0] == saved);
}
#[test]
fn returned_original_survives_replaced_draft_and_cannot_be_attached_to_another_record() {
    let t = Temp::new();
    let mut j = t.open();
    let d = draft(&mut j);
    let r = begin(&mut j, &d);
    let mut invalid_title = d.content.clone();
    invalid_title.title = "Changed title".into();
    assert!(j.save_draft(&d, invalid_title).is_err());
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&d));
    let mut next = d.content.clone();
    next.note[0]["elements"][0]["elements"][0]["text"] = json!("Changed note");
    assert!(next.note != d.content.note);
    let drafts::Mutation::Applied(changed) = j.save_draft(&d, next).unwrap() else {
        panic!("save")
    };
    let saved = j.direct_open_returned(&r, opened()).unwrap();
    assert!(saved.draft() == &d);
    assert!(j.draft(&d.scope).unwrap().as_ref() == Some(&changed));
    let mut forged = r.clone();
    forged.id = uuid::Uuid::new_v4().to_string();
    assert!(j.direct_open_returned(&forged, opened()).is_err());
    let mut wrong = info();
    wrong["channel"]["user"] = json!("U00000003");
    assert!(j.resolve_direct_open(&saved, wrong).is_err());
    let mut closed = info();
    closed["channel"].as_object_mut().unwrap().remove("is_open");
    assert!(j.resolve_direct_open(&saved, closed).is_err());
}
#[test]
fn corrupt_index_or_body_refuses_inventory_and_cannot_replace_original() {
    let t = Temp::new();
    let mut j = t.open();
    let d = draft(&mut j);
    let r = begin(&mut j, &d);
    j.conn
        .execute("UPDATE file_share_direct_opens SET peer='U00000003'", [])
        .unwrap();
    assert!(j.direct_opens(&d.scope.team, &d.scope.user).is_err());
    assert!(j.direct_open_returned(&r, opened()).is_err());
}
