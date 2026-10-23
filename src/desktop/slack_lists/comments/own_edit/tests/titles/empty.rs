use super::*;
use crate::profile_edit::sections::rich::search::file_source;
use super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo;

const FILE: &str = "FEDITEMPTY01";
fn tokens() -> Vec<Value> {
    vec![
        json!({"type":"canvas","file_id":FILE,"text":"Original fallback",
            "url":"https://fixture.invalid/original","style":{"bold":true},"opaque":1}),
        json!({"type":"canvas","file_id":FILE,"text":"Original fallback",
            "url":"https://fixture.invalid/original","style":{"italic":true},"opaque":2}),
    ]
}
fn raw(whitespace: bool) -> Value {
    json!({"id":FILE,"mode":"quip","filetype":"quip","created":101,
        "title":" \t\n","permalink":"https://fixture.invalid/canonical","opaque":{"keep":true},
        "title_blocks":if whitespace {json!([{"type":"rich_text","elements":[{"type":"rich_text_section",
            "elements":[{"type":"text","text":" \t\n","opaque":17}]}]}])} else {json!([])}})
}
fn publish(shell: &mut ShellState, value: Value) -> Result<(), Error> {
    let response =
        OrderedFileInfo::decode(&serde_json::to_vec(&json!({"ok":true,"file":value})).unwrap())
            .unwrap();
    file_source::FileInfoRead::capture(shell, "T1", FILE)?.publish(shell, &response)
}
async fn original_empty() -> (VirtualDom, Signal<ShellState>, Job, Local) {
    let (dom, mut state, mut job, local) = original().await;
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":tokens()}]}),
    ];
    job.durable.capture(&job.seed, &blocks).await.unwrap();
    let mut shell = state.write();
    shell.canvas_titles = crate::canvas::quip_titles::State::fresh();
    job.blocks = blocks.clone();
    job.tokens = FileTitles::reply_tokens(&shell, "T1", &blocks);
    let s = session_mut(&mut shell, &job.binding).unwrap();
    s.blocks = blocks;
    s.titles = Some(job.clone());
    assert!(job.active(&shell));
    drop(shell);
    (dom, state, job, local)
}
async fn acquire(
    state: Signal<ShellState>,
    job: &Job,
) -> Result<Arc<super::super::super::super::title_users::Observed>, Error> {
    let owner = job.clone();
    super::super::super::super::title_users::acquire_edit(
        state,
        "T1",
        "independent-edit-test",
        job.tokens.clone(),
        Arc::new(move |s| owner.active(s)),
    )
    .await
}

#[tokio::test]
async fn empty_canvas_edit_uses_own_receipt_with_duplicate_raw_occurrences_and_no_member_policy() {
    for whitespace in [false, true] {
        let (_dom, mut state, job, _local) = original_empty().await;
        assert_eq!(job.tokens, tokens());
        publish(&mut state.write(), raw(whitespace)).unwrap();
        let observed = acquire(state, &job).await.unwrap();
        let mut shell = state.write();
        assert!(observed.current(&shell));
        let proof = observed.snapshot(&shell);
        assert_eq!(proof["policy"], Value::Null);
        assert_eq!(proof["titles"]["users"], Value::Null);
        assert_eq!(proof["titles"]["tokens"], json!(tokens()));
        for file in proof["titles"]["files"].as_array().unwrap() {
            assert_eq!(file["source"], raw(whitespace));
            assert_eq!(file["receipt"], json!({"ok":true,"file":raw(whitespace)}));
        }
        assert_eq!(
            proof["titles"]["canvasFallback"][FILE]["descriptor"]["value"],
            "Untitled"
        );
        assert_eq!(
            proof["titles"]["canvasFallback"][FILE]["quipTitle"],
            if whitespace {
                json!({"fileId":FILE,"current":true,"value":"nullish"})
            } else {
                Value::Null
            }
        );
        let slot = session_mut(&mut shell, &job.binding)
            .unwrap()
            .titles
            .as_mut()
            .unwrap();
        slot.loading = false;
        slot.observed = Some(observed);
        assert!(presentation(&shell).unwrap().mount_ready);
        let labels = labels(&shell, &job.binding);
        assert_eq!(labels.len(), 2);
        assert!(labels.values().all(|label| label == "Untitled"));
        assert_eq!(session(&shell, &job.binding).unwrap().blocks, job.blocks);
        assert!(session(&shell, &job.binding).unwrap().native.is_none());
    }
}

#[tokio::test]
async fn cold_edit_requires_metadata_and_unsupported_response_preserves_original_fallback() {
    let (_dom, mut state, job, _local) = original_empty().await;
    assert!(!presentation(&state.peek()).unwrap().mount_ready);
    let mut unsupported = raw(false);
    unsupported["mode"] = json!("hosted");
    unsupported["filetype"] = json!("pdf");
    assert!(publish(&mut state.write(), unsupported).is_err());
    // Positive unsupported evidence is an outcome, not an Untitled grant.
    let observed = acquire(state, &job).await.unwrap();
    let mut shell = state.write();
    assert_eq!(
        observed.snapshot(&shell)["titles"]["excludedTokens"],
        json!(tokens())
    );
    let slot = session_mut(&mut shell, &job.binding)
        .unwrap()
        .titles
        .as_mut()
        .unwrap();
    slot.loading = false;
    slot.observed = Some(observed);
    assert!(presentation(&shell).unwrap().mount_ready);
    assert!(labels(&shell, &job.binding).is_empty());
    assert_eq!(session(&shell, &job.binding).unwrap().blocks, job.blocks);
}

#[tokio::test]
async fn malformed_metadata_keeps_exact_edit_and_cancel_without_authorizing_mount_or_save() {
    let (_dom, mut state, job, _local) = original_empty().await;
    let mut malformed = raw(false);
    malformed["id"] = json!("FOTHER01");
    assert!(publish(&mut state.write(), malformed).is_err());
    assert!(!FileTitles::reply_excluded(&state.peek(), "T1", FILE));
    assert!(
        FileTitles::capture_reply(&state.peek(), "T1", job.tokens.clone(), Arc::new(|_| true))
            .is_err()
    );
    let mut shell = state.write();
    let slot = session_mut(&mut shell, &job.binding)
        .unwrap()
        .titles
        .as_mut()
        .unwrap();
    slot.loading = false;
    slot.error =
        Some("Title details could not load. Your saved edit and reply are retained.".into());
    let view = presentation(&shell).unwrap();
    assert!(!view.mount_ready && !view.can_save && view.can_cancel && view.can_retry_titles);
    let retained = session(&shell, &job.binding).unwrap();
    assert_eq!(retained.blocks, job.blocks);
    assert!(retained.native.is_none() && retained.native_sequence == 0);
    assert!(Arc::ptr_eq(
        retained.durable.as_ref().unwrap(),
        &job.durable
    ));
    assert!(Arc::ptr_eq(&retained.suspension, &job.suspension));
}

#[tokio::test]
async fn original_empty_edit_receipt_refuses_seed_revision_and_file_replacement() {
    for mutation in 0..3 {
        let (_dom, mut state, job, _local) = original_empty().await;
        publish(&mut state.write(), raw(false)).unwrap();
        let observed = acquire(state, &job).await.unwrap();
        let mut shell = state.write();
        match mutation {
            0 => {
                session_mut(&mut shell, &job.binding).unwrap().seed = Arc::new((*job.seed).clone())
            }
            1 => session_mut(&mut shell, &job.binding).unwrap().revision += 1,
            _ => {
                let mut replacement = raw(false);
                replacement["title"] = json!("Successor metadata");
                publish(&mut shell, replacement).unwrap();
            }
        }
        assert!(!observed.current(&shell));
        assert!(observed.files.labels(&shell).is_err());
    }
}
