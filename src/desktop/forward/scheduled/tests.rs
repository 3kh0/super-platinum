use super::*;
use serde_json::json;

struct Fixture {
    directory: std::path::PathBuf,
    storage: storage::Storage,
    record: model::Record,
}
impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("scheduled-forward-host-{}", uuid::Uuid::new_v4()));
        let account = uuid::Uuid::new_v4().to_string();
        let owner = model::Owner {
            team: "T1".into(),
            user: "U1".into(),
        };
        let path = directory.join("journal.sqlite");
        let intent = model::Intent::new(owner.clone(), model::Source {
            channel:"C1".into(), timestamp:"100.000001".into(), thread:None, author:Some("U1".into()),
            permalink:"https://fixture.slack.com/archives/C1/p100000001".into(), private:false,
        }, model::Destination::Conversation("C2".into()), "Destination".into(),
        vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Preserved note"},{"type":"future_atom","opaque":[1,2]}]}]})], 2000, false, 1000).unwrap();
        let record = model::store::Journal::open(&path, &account)
            .unwrap()
            .create(intent, 1000)
            .unwrap();
        Self {
            directory,
            storage: storage::Storage::isolated(account, owner, path),
            record,
        }
    }
    fn registry(&self) -> Registry {
        let mut root = Registry::default();
        root.adopt(self.storage.clone(), vec![self.record.clone()]);
        root
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn history_page_cannot_discard_captured_input_or_unsaved_read_receipt() {
    let f = Fixture::new();
    let mut root = f.registry();
    let id = f.record.id().to_owned();
    root.entries.get_mut(&id).unwrap().pending_edit = Some((
        vec![json!({"type":"future","text":"new exact input"})],
        2500,
    ));
    root.adopt(f.storage.clone(), vec![]);
    assert_eq!(root.entries[&id].pending_edit.as_ref().unwrap().1, 2500);
    root.entries.get_mut(&id).unwrap().pending_edit = None;
    root.entries.get_mut(&id).unwrap().local = Some(storage::Local::Cooldown {
        expected: f.record.clone(),
        seconds: 45,
        at: 1100,
    });
    root.adopt(f.storage.clone(), vec![]);
    assert!(root.snapshots(&f.storage)[0].needs_storage);
    assert_eq!(root.entries[&id].record.as_ref(), Some(&f.record));
    root.entries.get_mut(&id).unwrap().local = None;
    root.adopt(f.storage.clone(), vec![]);
    assert!(
        root.entries.is_empty(),
        "settled old pages remain on disk rather than accumulating in memory"
    );
}

#[test]
fn exact_native_capture_claim_refuses_second_claim_and_old_cleanup() {
    let mut dom = VirtualDom::new(|| {
        let f = use_hook(|| Arc::new(Fixture::new()));
        let retained = f.clone();
        let mut book = use_signal(move || retained.registry());
        use_hook(move || {
            let owner = Owner {
                storage: f.storage.clone(),
                context: crate::scheduled::Context {
                    transport: Arc::new(Transport::disconnected_fixture("synthetic")),
                    client: SlackClient::default(),
                    session: WorkspaceSession {
                        team_id: "T1".into(),
                        enterprise_id: None,
                        user_id: "U1".into(),
                        name: "Fixture".into(),
                        url: "https://fixture.slack.com".into(),
                        token: "synthetic".into(),
                    },
                    epoch: 1,
                },
            };
            let first = controller::begin_capture(book, &owner, &f.record).unwrap();
            assert!(controller::begin_capture(book, &owner, &f.record).is_err());
            book.write().adopt(f.storage.clone(), vec![]);
            assert_eq!(
                book.peek().entries[f.record.id()].capture.as_deref(),
                Some(first.as_str())
            );
            controller::cancel_capture(book, f.record.id(), &first, "first bridge failed".into());
            let second = controller::begin_capture(book, &owner, &f.record).unwrap();
            assert_ne!(first, second);
            controller::cancel_capture(book, f.record.id(), &first, "obsolete".into());
            assert!(book.peek().entries[f.record.id()].busy);
            assert_eq!(
                book.peek().entries[f.record.id()].capture.as_deref(),
                Some(second.as_str())
            );
            controller::cancel_capture(book, f.record.id(), &second, "second failed".into());
            assert!(!book.peek().entries[f.record.id()].busy);
            assert_eq!(book.peek().entries[f.record.id()].error, "second failed");
        });
        rsx! { div {} }
    });
    dom.rebuild_in_place();
}

#[test]
fn retry_after_receipt_survives_local_open_failure_with_original_deadline() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let f = Fixture::new();
            let path = f.directory.join("journal.sqlite");
            let backup = f.directory.join("retained.sqlite");
            std::fs::rename(&path, &backup).unwrap();
            std::fs::create_dir(&path).unwrap();
            let receipt = storage::Local::Cooldown {
                expected: f.record.clone(),
                seconds: 60,
                at: 1100,
            };
            assert!(f.storage.local(receipt.clone()).await.is_err());
            std::fs::remove_dir(&path).unwrap();
            std::fs::rename(&backup, &path).unwrap();
            let saved = f.storage.local(receipt).await.unwrap();
            assert_eq!(saved.retry_at(), 1160);
            assert_eq!(saved.intent(), f.record.intent());
            let retained = f.storage.page(None).await.unwrap();
            assert_eq!(retained, vec![saved]);
        });
}

#[test]
fn captured_edit_has_storage_only_retry_under_its_original_owner() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let f = Fixture::new();
        let note = vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"exact replacement"},{"type":"future_atom","value":[3,1]}]}]})];
        let receipt = storage::Local::Edit {expected:f.record.clone(), note:note.clone(), time:2500, at:1100};
        let mut root = f.registry();
        let entry = root.entries.get_mut(f.record.id()).unwrap();
        entry.pending_edit = Some((note.clone(),2500));
        entry.local = Some(receipt.clone());
        assert!(root.snapshots(&f.storage)[0].needs_storage);
        let path = f.directory.join("journal.sqlite");
        let backup = f.directory.join("retained.sqlite");
        std::fs::rename(&path,&backup).unwrap();std::fs::create_dir(&path).unwrap();
        assert!(f.storage.local(receipt.clone()).await.is_err());
        assert_eq!(root.entries[f.record.id()].pending_edit.as_ref(), Some(&(note.clone(),2500)));
        assert!(root.snapshots(&f.storage)[0].needs_storage);
        std::fs::remove_dir(&path).unwrap();std::fs::rename(&backup,&path).unwrap();
        // A replacement account cannot adopt the original owner's private DB.
        let other = storage::Storage::isolated(uuid::Uuid::new_v4().to_string(),model::Owner {team:"T2".into(),user:"U2".into()},path);
        assert!(other.local(receipt.clone()).await.is_err());
        let saved = f.storage.local(receipt).await.unwrap();
        assert_eq!(saved.intent().owner(), f.record.intent().owner());
        assert_eq!(saved.intent().note(), note.as_slice());assert_eq!(saved.intent().post_at(),2500);
        assert_eq!(saved.phase(),model::Phase::Ready);
        assert_eq!(saved.revision(), f.record.revision()+1);
        assert_eq!(saved.reviewed(), f.record.reviewed());
        assert_eq!(f.storage.page(None).await.unwrap(),vec![saved]);
        // Local::Edit has only Journal capability; no request/transport is yielded.
    });
}

#[test]
fn discovery_cursor_is_bound_to_the_whole_durable_record_and_private_owner() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let f=Fixture::new();let expected=f.record.clone();
        let unknown=f.storage.run(move|journal|Ok(journal.reserve(&expected,false,None,1100)?.record().clone())).await.unwrap();
        let mut root=Registry::default();root.adopt(f.storage.clone(),vec![unknown.clone()]);
        let scan=model::discovery::Scan::new(&unknown).unwrap();
        let model::discovery::Page::More(scan)=scan.accept(&json!({"ok":true,"has_more":true,"drafts":[{"client_msg_id":"someone-else","last_updated_ts":"123.1"}]})).unwrap() else{panic!("next page")};
        root.entries.get_mut(unknown.id()).unwrap().discovery=Some(scan);
        assert!(root.snapshots(&f.storage)[0].continuing);
        // A persisted Retry-After produces a different whole record; its next
        // explicit read must start fresh, never reuse an older scan capability.
        let expected=unknown.clone();let newer=f.storage.run(move|journal|journal.defer_read(&expected,30,1101)).await.unwrap();
        root.adopt(f.storage.clone(),vec![newer]);assert!(!root.snapshots(&f.storage)[0].continuing);
        let other=storage::Storage::isolated(uuid::Uuid::new_v4().to_string(),model::Owner{team:"T2".into(),user:"U2".into()},f.directory.join("foreign.sqlite"));
        assert!(root.snapshots(&other).is_empty());
        assert_eq!(root.entries[unknown.id()].record.as_ref().unwrap().phase(),model::Phase::Unknown);
    });
}

#[test]
fn malformed_read_does_not_poison_storage_retry_but_valid_receipt_survives_disk_failure() {
    let mut dom = VirtualDom::new(|| {
        let f = use_hook(|| Arc::new(Fixture::new()));
        let mut book = use_signal(Registry::default);
        use_hook(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let expected=f.record.clone();
                let unknown=f.storage.run(move|j|Ok(j.reserve(&expected,false,None,1100)?.record().clone())).await.unwrap();
                book.write().adopt(f.storage.clone(),vec![unknown.clone()]);
                let owner=Owner{storage:f.storage.clone(),context:crate::scheduled::Context{
                    transport:Arc::new(Transport::disconnected_fixture("synthetic")),client:SlackClient::default(),
                    session:WorkspaceSession{team_id:"T1".into(),enterprise_id:None,user_id:"U1".into(),name:"Fixture".into(),url:"https://fixture.slack.com".into(),token:"synthetic".into()},epoch:1,
                }};
                let raw=json!({"ok":true,"draft":{"id":"Dr1","client_msg_id":unknown.intent().client_msg_id(),"blocks":unknown.intent().blocks(),"destinations":[{"channel_id":"C2"}],"file_ids":[],"date_scheduled":2000,"last_updated_ts":"1234","is_deleted":false,"is_sent":false}});
                let mut partial=raw.clone();partial["draft"].as_object_mut().unwrap().remove("blocks");
                let mut contradictory=raw.clone();contradictory["ok"]=json!(false);
                let mut foreign=raw.clone();foreign["draft"]["client_msg_id"]=json!(uuid::Uuid::new_v4().to_string());
                for invalid in [partial,contradictory,foreign] {
                    let result=controller::retain_local(book,&owner,unknown.id(),storage::Local::Review{expected:unknown.clone(),raw:invalid,at:1200}).await;
                    assert!(result.is_err());assert!(book.peek().entries[unknown.id()].local.is_none());
                    assert!(!book.peek().snapshots(&f.storage)[0].needs_storage);
                }
                let path=f.directory.join("journal.sqlite");let backup=f.directory.join("retained.sqlite");
                std::fs::rename(&path,&backup).unwrap();std::fs::create_dir(&path).unwrap();
                let receipt=storage::Local::Review{expected:unknown.clone(),raw,at:1200};
                assert!(controller::retain_local(book,&owner,unknown.id(),receipt.clone()).await.is_err());
                assert!(book.peek().entries[unknown.id()].local.is_some());assert!(book.peek().snapshots(&f.storage)[0].needs_storage);
                std::fs::remove_dir(&path).unwrap();std::fs::rename(&backup,&path).unwrap();
                let saved=controller::retain_local(book,&owner,unknown.id(),receipt).await.unwrap();
                assert_eq!(saved.phase(),model::Phase::Confirmed);assert!(book.peek().entries[unknown.id()].local.is_none());
            });
        });
        rsx! {div{}}
    });
    dom.rebuild_in_place();
}

#[test]
fn invalid_native_edit_remains_editable_instead_of_becoming_disk_retry() {
    let f = Fixture::new();
    let good = f.record.intent().note().to_vec();
    assert!(
        storage::Local::Edit {
            expected: f.record.clone(),
            note: good.clone(),
            time: 2500,
            at: 1100
        }
        .admissible()
    );
    for (note, time) in [
        (good.clone(), 0),
        (good.clone(), 1000),
        (good.clone(), i64::MAX),
        (vec![json!({"type":"rich_text","elements":false})], 2500),
    ] {
        let mut root = f.registry();
        root.entries.get_mut(f.record.id()).unwrap().pending_edit = Some((note.clone(), time));
        let receipt = storage::Local::Edit {
            expected: f.record.clone(),
            note: note.clone(),
            time,
            at: 1100,
        };
        assert!(!receipt.admissible());
        assert!(!root.snapshots(&f.storage)[0].needs_storage);
        assert_eq!(
            root.entries[f.record.id()].pending_edit.as_ref(),
            Some(&(note, time))
        );
        assert_eq!(root.entries[f.record.id()].record.as_ref(), Some(&f.record));
    }
}
