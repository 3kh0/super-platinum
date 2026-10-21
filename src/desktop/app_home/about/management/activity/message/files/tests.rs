use super::super::super::{ActivityDetailRequest, tests as activity_tests};
use super::*;
use serde_json::json;
fn row() -> Value {
    json!({"id":"FLIST","mode":"list","filetype":"list","created":1,"title":"R\u{e9}vision &amp; \u{4e2d} _list_ <Review>","permalink":"https://fixture.invalid/list"})
}
#[test]
fn default_list_title_is_two_pass_literal_text_without_profile_fallback() {
    let mut raw = row();
    assert_eq!(
        policy::label(&raw, Kind::List).unwrap().text,
        "R\u{e9}vision & \u{4e2d} _list_ <Review>"
    );
    raw["title"] = json!("&amp;lt; &eacute; &#x4e2d; &#39;");
    assert_eq!(
        policy::label(&raw, Kind::List).unwrap().text,
        "< &eacute; &#x4e2d; '"
    );
    raw["title"] = json!("");
    raw["name"] = json!("Must not replace List selector title");
    assert!(policy::label(&raw, Kind::List).is_none());
}
#[test]
fn default_list_and_canvas_have_distinct_bounded_entity_passes() {
    for (raw_title, list, canvas) in [
        (
            "&amp;lt;b&amp;gt; &amp;amp;lt;",
            "<b> &lt;",
            "&lt;b&gt; &amp;lt;",
        ),
        (
            "&amp;amp; &amp;lt; &amp;gt; &amp;quot; &amp;#39;",
            "& < > \" '",
            "&amp; &lt; &gt; &quot; &#39;",
        ),
        (
            "&AMP; &nbsp; &#65; &#x41; &#039;",
            "&AMP; &nbsp; &#65; &#x41; &#039;",
            "&AMP; &nbsp; &#65; &#x41; &#039;",
        ),
        (
            " \tR\u{e9}vision \u{4e2d} &amp;lt;Review&amp;gt;\n ",
            " \tR\u{e9}vision \u{4e2d} <Review>\n ",
            " \tR\u{e9}vision \u{4e2d} &lt;Review&gt;\n ",
        ),
    ] {
        let mut raw = row();
        raw["title"] = json!(raw_title);
        assert_eq!(policy::label(&raw, Kind::List).unwrap().text, list);
        raw["mode"] = json!("quip");
        raw["filetype"] = json!("quip");
        assert_eq!(policy::label(&raw, Kind::Canvas).unwrap().text, canvas);
    }
}

#[test]
fn final_list_semantic_label_feeds_emoji_without_a_third_decode() {
    use crate::media::MediaUri;
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    let w = s.core.workspaces.get_mut("T1").unwrap();
    for (name, value) in [
        ("wave", "https://fixture.invalid/collision.png"),
        ("encoded_ready", "https://fixture.invalid/ready.png"),
        ("encoded_alias", "alias:encoded_ready"),
    ] {
        w.custom_emoji.insert(
            name.into(),
            serde_json::from_value(json!({"name":name,"value":value})).unwrap(),
        );
    }
    let Inline::Emoji {
        image: Some(id), ..
    } = single(&s, "T1", ":encoded_alias:", true)
    else {
        panic!("original alias must resolve");
    };
    s.media
        .insert(id.clone(), "image/svg+xml", b"<svg/>".to_vec());
    let mut raw = row();
    raw["title"] = json!("&amp;lt;b&amp;gt; &amp;amp;lt; :wave: :encoded_alias: :unknown:");
    let original = raw.clone();
    let label = policy::label(&raw, Kind::List).unwrap();
    let painted = title::Painted::project(&s, "T1", &label.text, s.media_epoch);
    assert_eq!(painted.label, "<b> &lt; :wave: :encoded_alias: :unknown:");
    assert_eq!(
        serde_json::to_value(&painted.fragments).unwrap(),
        json!([
            {"kind":"text","text":"<b> &lt; "},
            {"kind":"emoji","text":"\u{1f44b}","title":":wave:"},
            {"kind":"text","text":" "},
            {"kind":"image","src":id.webview_uri_at(s.media_epoch),"alt":":encoded_alias:","title":":encoded_alias:"},
            {"kind":"text","text":" "},
            {"kind":"text","text":":unknown:"}
        ])
    );
    assert!(painted.current(&s, "T1"));
    assert_eq!(raw, original);
}

#[test]
fn decoded_activity_label_does_not_rewrite_canonical_title_or_receipt() {
    let (mut s, visit) = scene();
    let source =
        Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
    let team = source.team().to_owned();
    let mut raw = row();
    raw["title"] = json!("R\u{e9}vision &amp;lt;b&amp;gt; &amp;amp;lt;");
    raw["opaque"] = json!({"retain":[false,0,null,"&amp;lt;"]});
    let received =
        OrderedFileInfo::decode(json!({"ok":true,"file":raw}).to_string().as_bytes()).unwrap();
    FileInfoRead::capture(&mut s, &team, "FLIST")
        .unwrap()
        .publish(&mut s, &received)
        .unwrap();
    let before = FileObservation::capture(&s, &team, "FLIST").unwrap();
    assert_eq!(before.source()["title"], raw["title"]);
    assert_eq!(before.effective()["title"], raw["title"]);
    assert_eq!(
        project(&s, &team, "FLIST", Kind::List).unwrap().text,
        "R\u{e9}vision <b> &lt;"
    );
    let after = FileObservation::capture(&s, &team, "FLIST").unwrap();
    assert_eq!(after.source(), before.source());
    assert_eq!(after.effective(), before.effective());
    assert_eq!(after.receipt(), before.receipt());
    assert_eq!(received.file(), &raw);
    assert!(before.current(&s) && source.current(&s));
}
#[test]
fn ordinary_kind_and_status_are_not_inferred_from_activity_token() {
    for (key, value) in [
        ("mode", json!("hosted")),
        ("filetype", json!("png")),
        ("is_deleted", json!(true)),
        ("file_access", json!("access_denied")),
        ("subtype", json!("poll")),
        ("isUnknown", json!(true)),
        (
            "product_icon",
            json!({"url":"https://fixture.invalid/icon"}),
        ),
    ] {
        let mut raw = row();
        if key == "mode" || key == "filetype" {
            raw["mode"] = json!("hosted");
            raw["filetype"] = json!("png");
        } else {
            raw[key] = value;
        }
        assert!(policy::label(&raw, Kind::List).is_none(), "{key}");
    }
    assert!(policy::label(&row(), Kind::Canvas).is_none());
}
#[test]
fn static_canvas_title_does_not_flatten_rich_title_or_invent_link() {
    let mut raw = row();
    raw["mode"] = json!("quip");
    raw["filetype"] = json!("quip");
    raw["title"] = json!("Canvas &amp; caf\u{e9} \u{4e2d}");
    assert_eq!(
        policy::label(&raw, Kind::Canvas).unwrap().text,
        "Canvas & caf\u{e9} \u{4e2d}"
    );
    raw["title_blocks"] = json!([]);
    assert!(policy::label(&raw, Kind::Canvas).is_none());
}
fn scene() -> (ShellState, u64) {
    let (mut s, visit) = activity_tests::shell();
    let d = document_mut(&mut s, visit).unwrap();
    d.activity.detail_row = Some(json!({"workflow_execution_id":"EXECUTION"}));
    d.activity.detail=Some(ActivityDetailRequest::new("WF_REVIEW","EXECUTION").unwrap().decode(json!({"ok":true,"workflow_event":{"workflow_execution_id":"EXECUTION","trigger_message":{"template":"Open {l} then {l} and {c}","variables":{"l":{"type":"LIST","value":"FLIST"},"c":{"type":"CANVAS","value":"FCANVAS"}}},"step_events":[]}})).unwrap());
    (s, visit)
}
#[test]
fn repeated_ids_bind_exact_original_occurrence_and_kind() {
    let (s, visit) = scene();
    let a = Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
    let b = Source::capture_file(&s, visit, Location::Trigger, 3, "FLIST", Kind::List).unwrap();
    assert_ne!(a.button(), b.button());
    assert!(a.current(&s) && b.current(&s));
    assert!(Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::Canvas).is_none());
    assert!(Source::capture_file(&s, visit, Location::Trigger, 0, "FLIST", Kind::List).is_none());
}
#[test]
fn old_file_occurrence_cannot_borrow_changed_draft_filter_or_detail() {
    for change in 0..4 {
        let (mut s, visit) = scene();
        let original =
            Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
        let d = document_mut(&mut s, visit).unwrap();
        match change {
            0 => d.title.push('x'),
            1 => d.activity.filters.user.push('x'),
            2 => d.activity.detail_revision = next(),
            _ => d.activity.member_composing = true,
        };
        assert!(!original.current(&s));
    }
}
#[test]
fn canonical_read_preserves_document_and_invalidation_exposes_explicit_retry() {
    let (mut s, visit) = scene();
    let source =
        Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
    let team = source.team().to_owned();
    let d = document(&s, visit).unwrap();
    let before = (
        d.id,
        d.revision,
        d.title.clone(),
        d.description.clone(),
        d.editor.responses().clone(),
        d.pending,
    );
    let read = FileInfoRead::capture(&mut s, &team, "FLIST").unwrap();
    let received =
        OrderedFileInfo::decode(json!({"ok":true,"file":row()}).to_string().as_bytes()).unwrap();
    read.publish(&mut s, &received).unwrap();
    let observed = FileObservation::capture(&s, &team, "FLIST").unwrap();
    assert!(observed.current(&s));
    assert!(!retryable("ready", true));
    crate::profile_edit::sections::rich::search::file_source::invalidate(&mut s, &team, "FLIST");
    assert!(!observed.current(&s));
    assert!(FileObservation::capture(&s, &team, "FLIST").is_err());
    assert!(retryable("ready", false));
    assert!(retryable("retired", false));
    assert!(!retryable("loading", false));
    assert!(source.current(&s));
    assert!(FileInfoRead::capture(&mut s, &team, "FLIST").is_ok());
    let d = document(&s, visit).unwrap();
    assert_eq!(
        before,
        (
            d.id,
            d.revision,
            d.title.clone(),
            d.description.clone(),
            d.editor.responses().clone(),
            d.pending
        )
    );
}

#[test]
fn read_only_lock_is_not_the_source_locked_status_predicate() {
    let mut raw = row();
    raw["is_locked"] = json!(true);
    raw["access"] = json!("read");
    assert!(policy::label(&raw, Kind::List).is_some());
    raw["file_access"] = json!("not_visible");
    assert!(policy::label(&raw, Kind::List).is_none());
}

#[test]
fn retained_file_callback_is_retired_by_calendar_open_dirty_and_cancel_epoch() {
    for change in 0..3 {
        let (mut s, visit) = scene();
        let original =
            Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
        let d = document_mut(&mut s, visit).unwrap();
        match change {
            0 => d.activity.calendar.open = true,
            1 => d.activity.calendar.dirty = true,
            _ => d.activity.calendar.epoch = next(),
        };
        assert!(!original.current(&s), "calendar mutation {change}");
        if change < 2 {
            assert!(
                Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List)
                    .is_none()
            );
        }
        let d = document_mut(&mut s, visit).unwrap();
        d.activity.calendar.open = false;
        d.activity.calendar.dirty = false;
        d.activity.calendar.epoch = next();
        assert!(!original.current(&s));
        assert!(
            Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List)
                .unwrap()
                .current(&s)
        );
    }
}

#[tokio::test]
async fn metadata_retirement_drops_original_held_receiver() {
    use std::cell::Cell;
    let current = Cell::new(true);
    let entered = Cell::new(false);
    let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
    let held = acquire::guarded(
        async {
            entered.set(true);
            receiver.await
        },
        || current.get(),
    );
    let retire = async {
        while !entered.get() {
            tokio::task::yield_now().await;
        }
        assert!(!sender.is_closed());
        current.set(false);
    };
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(2), async { tokio::join!(held, retire) })
            .await
            .expect("original held metadata read must retire within its bounded poll");
    assert!(result.is_none());
    assert!(sender.is_closed());
    assert!(sender.send(()).is_err());
}

#[tokio::test]
async fn metadata_completion_rechecks_owner_after_same_poll_response() {
    use std::cell::Cell;
    let current = Cell::new(true);
    let response = json!({"ok":true,"file":row()});
    let refused = acquire::guarded(
        async {
            current.set(false);
            response.clone()
        },
        || current.get(),
    )
    .await;
    assert!(refused.is_none());
    current.set(true);
    assert_eq!(
        acquire::guarded(async { response.clone() }, || current.get()).await,
        Some(response)
    );
}

#[test]
fn sibling_metadata_publication_requires_recapture_without_changing_workflow_draft() {
    let (mut s, visit) = scene();
    let source =
        Source::capture_file(&s, visit, Location::Trigger, 1, "FLIST", Kind::List).unwrap();
    let team = source.team().to_owned();
    let before = {
        let d = document(&s, visit).unwrap();
        (
            d.id,
            d.revision,
            d.title.clone(),
            d.description.clone(),
            d.editor.responses().clone(),
            d.pending,
            serde_json::to_value(&d.activity.filters).unwrap(),
            d.activity.detail_revision,
            d.activity.detail.as_ref().unwrap().raw().clone(),
        )
    };
    let mut canvas = row();
    canvas["id"] = json!("FCANVAS");
    canvas["mode"] = json!("quip");
    canvas["filetype"] = json!("quip");
    canvas["title"] = json!("Canvas &amp; caf\u{e9}");
    let canvas_response =
        OrderedFileInfo::decode(json!({"ok":true,"file":canvas}).to_string().as_bytes()).unwrap();
    FileInfoRead::capture(&mut s, &team, "FCANVAS")
        .unwrap()
        .publish(&mut s, &canvas_response)
        .unwrap();
    let old_canvas = FileObservation::capture(&s, &team, "FCANVAS").unwrap();
    let list_response =
        OrderedFileInfo::decode(json!({"ok":true,"file":row()}).to_string().as_bytes()).unwrap();
    FileInfoRead::capture(&mut s, &team, "FLIST")
        .unwrap()
        .publish(&mut s, &list_response)
        .unwrap();
    assert!(!old_canvas.current(&s));
    let new_canvas = FileObservation::capture(&s, &team, "FCANVAS").unwrap();
    let list = FileObservation::capture(&s, &team, "FLIST").unwrap();
    assert!(new_canvas.current(&s) && list.current(&s));
    assert_eq!(new_canvas.source(), old_canvas.source());
    assert_eq!(new_canvas.receipt(), old_canvas.receipt());
    assert!(project(&s, &team, "FCANVAS", Kind::Canvas).is_some());
    assert!(project(&s, &team, "FLIST", Kind::List).is_some());
    assert!(source.current(&s));
    let d = document(&s, visit).unwrap();
    assert_eq!(
        before,
        (
            d.id,
            d.revision,
            d.title.clone(),
            d.description.clone(),
            d.editor.responses().clone(),
            d.pending,
            serde_json::to_value(&d.activity.filters).unwrap(),
            d.activity.detail_revision,
            d.activity.detail.as_ref().unwrap().raw().clone(),
        )
    );
}
