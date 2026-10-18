use super::*;
use super_platinum_core::slack::SlackClient;

struct Case {
    root: PathBuf,
    journal: model::store::Journal,
    record: model::Record,
    workspace: WorkspaceSession,
    client: SlackClient,
    // Declared after Journal so Windows closes SQLite before removing this tree.
    _cleanup: Cleanup,
}
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Case {
    fn new(person: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("scheduled-forward-server-{}", uuid::Uuid::new_v4()));
        let mut journal =
            model::store::Journal::open(root.join("journal.sqlite"), ACCOUNT).unwrap();
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Isolated fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: TOKEN.into(),
        };
        let intent=model::Intent::new(model::Owner{team:"T1".into(),user:"U00000001".into()},
            model::Source{channel:"C5".into(),timestamp:"100.000001".into(),thread:None,
                author:Some("U00000002".into()),permalink:"https://fixture.slack.com/archives/C5/p100000001".into(),private:true},
            if person {model::Destination::Person("U00000991".into())}else{model::Destination::Conversation("C2".into())},
            "Native destination".into(),vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Native note 日本語","style":{"bold":true}}]}]})],2000,true,1000).unwrap();
        let record = journal.create(intent, 1000).unwrap();
        Self {
            _cleanup: Cleanup(root.clone()),
            root,
            journal,
            record,
            workspace,
            client: SlackClient::default(),
        }
    }
    fn create(&mut self, server: &mut Server, scenario: &str) -> model::Record {
        let mut attempt = self
            .journal
            .reserve(&self.record, false, None, 1001)
            .unwrap();
        let request = attempt.take_request(&self.client, &self.workspace).unwrap();
        assert!(attempt.take_request(&self.client, &self.workspace).is_err());
        let completion = attempt.complete(server.execute(request, scenario, "exercise"), 1002);
        self.journal.finish(&completion).unwrap()
    }
}

#[test]
fn actual_single_attempt_request_preserves_rich_private_and_person_wire() {
    for person in [false, true] {
        let mut c = Case::new(person);
        let mut server = Server::default();
        let confirmed = c.create(&mut server, "ordered-rich");
        assert_eq!(confirmed.phase(), model::Phase::Confirmed);
        assert_eq!(server.writes, 1);
        assert_eq!(server.ledger.len(), 1);
        let raw = confirmed.reviewed().unwrap();
        assert_eq!(
            raw["draft"]["destinations"],
            if person {
                json!([{"user_ids":["U00000991"]}])
            } else {
                json!([{"channel_id":"C2"}])
            }
        );
        assert_eq!(
            raw["draft"]["unfurl"][0]["acknowledged_private_share"],
            true
        );
        assert_eq!(
            raw["draft"]["blocks"],
            serde_json::to_value(confirmed.intent().blocks()).unwrap()
        );
        let restored = model::store::Journal::open(c.root.join("journal.sqlite"), ACCOUNT)
            .unwrap()
            .get(confirmed.id())
            .unwrap()
            .unwrap();
        assert_eq!(restored, confirmed);
    }
}

#[test]
fn lost_create_needs_explicit_second_page_then_exact_info_without_a_second_write() {
    let mut c = Case::new(false);
    let mut server = Server::default();
    let unknown = c.create(&mut server, "lost-readback");
    assert_eq!(unknown.phase(), model::Phase::Unknown);
    let scan = model::discovery::Scan::new(&unknown).unwrap();
    let first = server
        .execute(
            scan.request(&c.client, &c.workspace).unwrap(),
            "lost-readback",
            "exercise",
        )
        .unwrap();
    let model::discovery::Page::More(next) = scan.accept(&first).unwrap() else {
        panic!("Expected manual continuation")
    };
    assert_eq!(server.ledger.len(), 2);
    assert_eq!(server.writes, 1);
    let second = server
        .execute(
            next.request(&c.client, &c.workspace).unwrap(),
            "lost-readback",
            "exercise",
        )
        .unwrap();
    let model::discovery::Page::Found(id) = next.accept(&second).unwrap() else {
        panic!("Expected unique identity")
    };
    let request = api::drafts_info(&c.client, &c.workspace, &id).unwrap();
    let raw = server
        .execute(request, "lost-readback", "exercise")
        .unwrap();
    let confirmed = c.journal.review(&unknown, &raw, 1003).unwrap();
    assert_eq!(confirmed.phase(), model::Phase::Confirmed);
    assert_eq!(server.writes, 1);
    assert_eq!(server.ledger.len(), 4);
}

#[test]
fn malformed_or_restart_requests_cannot_mutate_fixture_state() {
    let mut c = Case::new(false);
    let mut server = Server::default();
    let mut attempt = c.journal.reserve(&c.record, false, None, 1001).unwrap();
    let request = attempt.take_request(&c.client, &c.workspace).unwrap();
    let mut bad = request.clone();
    if let RequestBody::Form(fields) = &mut bad.body {
        fields.push(("token".into(), TOKEN.into()));
    }
    assert!(server.execute(bad, "ordered-rich", "exercise").is_err());
    assert!(
        server
            .execute(request.clone(), "ordered-rich", "restart")
            .is_err()
    );
    let mut missing = request.clone();
    missing.url = missing.url.replace("drafts.create", "drafts.update");
    assert!(server.execute(missing, "ordered-rich", "exercise").is_err());
    assert!(server.rows.is_empty());
    assert!(server.ledger.is_empty());
    assert_eq!(server.writes, 0);
    assert!(server.execute(request, "ordered-rich", "exercise").is_ok());
}

#[test]
fn explicit_remote_conflict_retains_full_original_and_only_injects_once() {
    let mut c = Case::new(false);
    let mut server = Server::default();
    c.create(&mut server, "conflict-review");
    let before = server.rows["DrScheduledForward"].clone();
    server.inject_conflict().unwrap();
    assert!(server.inject_conflict().is_err());
    assert_eq!(server.external.len(), 1);
    assert_eq!(server.external[0]["before"], before);
    assert_eq!(
        server.external[0]["after"],
        server.rows["DrScheduledForward"]
    );
    assert_eq!(server.writes, 1);
    assert_eq!(server.ledger.len(), 1);
}
