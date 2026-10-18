use super::*;
use serde_json::json;
use super_platinum_core::file_sharing::{
    drafts as d,
    store::{self, Journal},
};
const ACCOUNT: &str = "4b02f26e-393b-4fd1-a7da-5d11f76296ae";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("sp-late-direct-{}-{epoch}-{n}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn storage(&self) -> recovery::Storage {
        recovery::Storage::at(
            admission::Key {
                account: ACCOUNT.into(),
                team: "T00000001".into(),
                user: "U99999999".into(),
            },
            self.0.join("private.sqlite"),
        )
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn original(storage: &recovery::Storage, n: usize) -> Record {
    storage.run(move|j|{
        let scope=d::DraftScope{team:"T00000001".into(),user:"U99999999".into(),file:format!("F{n:08}")};
        let content=d::Content{source:json!({"id":scope.file,"mode":"hosted","is_external":false}),title:"Original file".into(),note:vec![],destination:None};
        let d::Claim::Claimed(draft)=j.claim_draft(scope,None,content)? else {panic!("claim")};
        let mut content=draft.content.clone();content.destination=Some(d::Destination{id:format!("person:U{n:08}"),name:"Person".into(),private:true});
        let d::Mutation::Applied(draft)=j.save_draft(&draft,content)? else {panic!("save")};
        let store::direct_open::Begin::Created(record)=j.begin_direct_open(&draft,json!({"ok":true,"results":[{"id":format!("U{n:08}"),"deleted":false,"is_bot":false}],"failed_ids":[]}),1)? else {panic!("open")};Ok(record)
    }).await.unwrap()
}
async fn completed(storage: &recovery::Storage, record: &Record, n: usize) {
    let record = record.clone();
    storage.run(move|j|{
        j.resolve_direct_open(&record,json!({"ok":true,"channel":{"id":"D00000001","user":record.peer(),"is_im":true,"is_open":true,"is_ext_shared":false}}))?;
        let mut content=record.draft().content.clone();content.destination=Some(d::Destination{id:"D00000001".into(),name:"Person".into(),private:true});
        let d::Mutation::Applied(draft)=j.save_draft(record.draft(),content)? else {panic!("resolve")};
        let store::Begin::Started(receipt)=j.begin_from_draft(&draft,2)? else {panic!("share")};
        j.finish_result(&receipt.scope,&receipt.attempt_id,Ok(json!({"ok":true})),n as u64+100)?;Ok(())
    }).await.unwrap();
}
fn raw() -> Value {
    json!({"ok":true,"channel":{"id":"D00000001"},"future":17})
}
#[tokio::test]
async fn actual_late_result_storage_releases_only_its_slot_for_repeated_workflows() {
    let temp = Temp::new();
    let storage = temp.storage();
    let custody = Arc::new(std::sync::Mutex::new(recovery::Custody::default()));
    for n in 1..=96 {
        let record = original(&storage, n).await;
        reserve(&custody, &storage, &record).unwrap();
        completed(&storage, &record, n).await;
        // The original worker is stopped by Completed, never allowed to Share
        // again; its matching returned slot is nevertheless released.
        assert!(matches!(
            returned(&custody, &record, Ok(raw())).await,
            Err(Refusal::Changed)
        ));
        assert!(custody.lock().unwrap().direct_results.is_empty());
    }
}
#[tokio::test]
async fn pruned_history_remains_visible_and_explicit_exact_discard_preserves_uncertain_records() {
    let temp = Temp::new();
    let storage = temp.storage();
    let custody = Arc::new(std::sync::Mutex::new(recovery::Custody::default()));
    let record = original(&storage, 1).await;
    reserve(&custody, &storage, &record).unwrap();
    completed(&storage, &record, 1).await;
    for n in 2..=store::MAX_TERMINAL + 2 {
        let next = original(&storage, n).await;
        completed(&storage, &next, n).await;
    }
    let uncertain = original(&storage, 1000).await;
    assert!(matches!(
        returned(&custody, &record, Ok(raw())).await,
        Err(Refusal::Storage)
    ));
    let mut held = custody.lock().unwrap();
    let rows = captured(&held, &storage.key);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].record == record);
    assert!(rows[0].raw.as_ref() == Some(&raw()));
    let mut foreign = rows[0].clone();
    foreign.identity = Arc::new(());
    assert!(!remove_exact(&mut held, &foreign));
    assert_eq!(held.direct_results.len(), 1);
    assert!(remove_exact(&mut held, &rows[0]));
    assert!(held.direct_results.is_empty());
    drop(held);
    let j = Journal::open(temp.0.join("private.sqlite"), ACCOUNT).unwrap();
    assert!(
        j.direct_opens(&storage.key.team, &storage.key.user)
            .unwrap()
            .as_slice()
            == [uncertain]
    );
    drop(j);
}
