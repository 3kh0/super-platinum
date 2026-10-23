use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo;

fn token() -> Value {
    json!({"type":"canvas","file_id":FILE,"text":"Original fallback",
        "url":"https://fixture.invalid/original","style":{"italic":true},"opaque":[1,2]})
}
fn blocks(token: &Value) -> Vec<Value> {
    vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[token]}]})]
}
fn raw(whitespace_blocks: bool) -> Value {
    let mut raw = file(FILE);
    raw["title"] = json!(" \t\n");
    raw["title_blocks"] = if whitespace_blocks {
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":" \t\n"}]}]}])
    } else {
        json!([])
    };
    raw
}
fn publish(s: &mut ShellState, raw: &Value) -> Result<(), Error> {
    let response =
        OrderedFileInfo::decode(&serde_json::to_vec(&json!({"ok":true,"file":raw})).unwrap())
            .unwrap();
    file_source::FileInfoRead::capture(s, TEAM, FILE)?
        .publish(s, &response)
        .map(|_| ())
}
fn complete(s: &ShellState) -> FileTitles {
    FileTitles::capture_reply(s, TEAM, vec![token()], Arc::new(|_| true))
        .unwrap()
        .complete(s, None, None, Arc::new(|_| true))
        .unwrap()
}

#[test]
fn cold_reply_elects_only_closed_whole_canvas_without_claiming_a_title() {
    let s = shell();
    let original = token();
    assert_eq!(
        FileTitles::reply_tokens(&s, TEAM, &blocks(&original)),
        [original.clone()]
    );
    assert!(
        FileTitles::capture_reply(&s, TEAM, vec![original.clone()], Arc::new(|_| true)).is_err()
    );
    assert!(!FileTitles::reply_excluded(&s, TEAM, FILE));
    for (key, value) in [
        ("type", json!("file")),
        ("section_id", json!("part")),
        ("record_id", json!("record")),
        ("is_skill_invocation", json!(true)),
        ("file_id", json!("bad/id")),
    ] {
        let mut unsupported = original.clone();
        unsupported[key] = value;
        let mut mixed = blocks(&original);
        mixed.extend(blocks(&unsupported));
        assert_eq!(
            FileTitles::reply_tokens(&s, TEAM, &mixed),
            [original.clone()]
        );
    }
}

#[test]
fn actual_cold_file_receipt_enables_independent_reply_context_without_query_authority() {
    for whitespace in [false, true] {
        let mut s = shell();
        s.self_account.user_id = "USELF01".into();
        s.canvas_titles = crate::canvas::quip_titles::State::fresh();
        let original = token();
        publish(&mut s, &raw(whitespace)).unwrap();
        let titles = complete(&s);
        assert!(titles.current(&s));
        assert_eq!(titles.label(&s, &original).unwrap(), "Untitled");
        assert!(titles.required().is_empty());
        let proof = titles.snapshot(&s);
        assert_eq!(proof["tokens"], json!([original]));
        assert_eq!(proof["files"][0]["source"], raw(whitespace));
        assert_eq!(
            proof["files"][0]["receipt"],
            json!({"ok":true,"file":raw(whitespace)})
        );
        assert_eq!(proof["users"], Value::Null);
        assert_eq!(
            proof["canvasFallback"][FILE]["quipTitle"],
            if whitespace {
                json!({"fileId":FILE,"current":true,"value":"nullish"})
            } else {
                Value::Null
            }
        );
        // Own-edit and absent-context consumers remain conservative.
        assert!(FileTitles::dependent_tokens(&s, TEAM, &blocks(&token())).is_empty());
        assert!(
            FileTitles::capture(&s, TEAM, vec![token()], Arc::new(|_| true))
                .unwrap()
                .complete(&s, None, None, Arc::new(|_| true))
                .is_err()
        );
    }
}

#[test]
fn positively_unsupported_cold_response_releases_only_label_gate_preserving_raw_token() {
    for hosted in [false, true] {
        let mut s = shell();
        let original = token();
        assert_eq!(
            FileTitles::reply_tokens(&s, TEAM, &blocks(&original)),
            [original.clone()]
        );
        let mut response = raw(false);
        if hosted {
            response["mode"] = json!("hosted");
            response["filetype"] = json!("pdf");
            assert!(publish(&mut s, &response).is_err());
            assert!(FileTitles::reply_excluded(&s, TEAM, FILE));
        } else {
            response["title_blocks"] = json!([{"type":"future_title","opaque":17}]);
            publish(&mut s, &response).unwrap();
        }
        assert!(FileTitles::reply_tokens(&s, TEAM, &blocks(&original)).is_empty());
        let titles = complete(&s);
        assert!(titles.current(&s));
        assert_eq!(titles.labels(&s).unwrap(), BTreeMap::new());
        assert!(titles.label(&s, &original).is_err());
        assert_eq!(titles.snapshot(&s)["excludedTokens"], json!([original]));
        assert_eq!(
            file_source::Snapshot::capture(&s, TEAM)
                .unwrap()
                .source(FILE),
            Some(&response)
        );
        crate::file_events::invalidate(&mut s, TEAM, FILE);
        assert!(!titles.current(&s));
        assert_eq!(
            FileTitles::reply_tokens(&s, TEAM, &blocks(&token())),
            [token()]
        );
    }
}

#[test]
fn missing_or_malformed_read_never_becomes_positive_unsupported_evidence() {
    let mut s = shell();
    let read = file_source::FileInfoRead::capture(&mut s, TEAM, FILE).unwrap();
    let wrong = OrderedFileInfo::decode(
        &serde_json::to_vec(&json!({"ok":true,"file":{"id":"FOTHER01","mode":"hosted"}})).unwrap(),
    )
    .unwrap();
    assert!(read.publish(&mut s, &wrong).is_err());
    assert!(!FileTitles::reply_excluded(&s, TEAM, FILE));
    assert!(FileTitles::capture_reply(&s, TEAM, vec![token()], Arc::new(|_| true)).is_err());
    assert_eq!(
        FileTitles::reply_tokens(&s, TEAM, &blocks(&token())),
        [token()]
    );
}

#[test]
fn original_reply_owner_and_file_receipt_refuse_late_or_replaced_completion() {
    for replacement in [false, true] {
        let mut s = shell();
        publish(&mut s, &raw(false)).unwrap();
        let active = Arc::new(AtomicBool::new(true));
        let guard = active.clone();
        let titles = FileTitles::capture_reply(
            &s,
            TEAM,
            vec![token()],
            Arc::new(move |_| guard.load(Ordering::SeqCst)),
        )
        .unwrap();
        if replacement {
            let mut next = raw(false);
            next["title"] = json!("Successor title");
            publish(&mut s, &next).unwrap();
        } else {
            active.store(false, Ordering::SeqCst);
        }
        assert!(!titles.current(&s));
        assert!(titles.complete(&s, None, None, Arc::new(|_| true)).is_err());
    }
}
