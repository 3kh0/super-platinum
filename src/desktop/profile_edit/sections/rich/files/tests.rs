use super::*;
use serde_json::json;
mod channels;
mod deleted;
mod events;
mod groups;
mod reference;
mod sections;

fn setup() -> (ShellState, Token) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_interaction(&mut shell);
    let token = Token::capture(&shell).unwrap();
    (shell, token)
}
fn response(id: &str, title: &str, mode: &str) -> Value {
    json!({"ok":true,"file":{"id":id,"mode":mode,"filetype":mode,"created":1,"title":title}})
}
fn put(shell: &mut ShellState, token: &Token, raw: &Value) {
    install(
        shell,
        token,
        raw["file"]["id"].as_str().unwrap(),
        socket(shell, token).unwrap(),
        crate::file_events::snapshot_revision(shell),
        raw,
    )
    .unwrap();
}
#[test]
fn singleton_info_uses_original_defaults_without_search_options() {
    let ctx = crate::scheduled::Context {
        client: Default::default(),
        transport: Arc::new(
            super_platinum_core::slack::Transport::new("synthetic-cookie").unwrap(),
        ),
        epoch: 1,
        session: super_platinum_core::config::WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.invalid".into(),
            token: "synthetic".into(),
        },
    };
    let req = request(&ctx, "FFILE0001");
    assert!(req.url.split('?').next().unwrap().ends_with("/files.info"));
    let super_platinum_core::slack::client::RequestBody::Form(body) = req.body else {
        panic!("form request");
    };
    let mut body: BTreeMap<_, _> = body.into_iter().collect();
    body.remove("token");
    assert_eq!(
        body,
        BTreeMap::from([
            ("file".into(), "FFILE0001".into()),
            ("page".into(), "1".into()),
            ("count".into(), "500".into()),
            ("truncate".into(), "true".into()),
        ])
    );
}
#[test]
fn title_observations_bind_socket_source_and_original_kind() {
    let (mut shell, token) = setup();
    let raw = response("FFILE0001", "Canonical title", "quip");
    put(&mut shell, &token, &raw);
    let observed = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    let wire =
        json!({"type":"canvas","file_id":"FFILE0001","text":"wire URL is not a title","opaque":42});
    assert_eq!(observed.label(&wire).unwrap(), "Canonical title");
    assert!(
        observed
            .label(&json!({"type":"file","file_id":"FFILE0001"}))
            .is_err()
    );
    put(
        &mut shell,
        &token,
        &response("FFILE0002", "Unrelated title", "list"),
    );
    assert!(observed.current(&shell));
    put(
        &mut shell,
        &token,
        &response("FFILE0001", "Renamed title", "quip"),
    );
    assert!(!observed.current(&shell));
    let next = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(next.label(&wire).unwrap(), "Renamed title");
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .rt_generation += 1;
    assert!(!next.current(&shell));
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
}
#[test]
fn explicit_invalidation_keeps_preimage_and_stale_opening_cannot_mount() {
    let (mut shell, token) = setup();
    let id = "FFILE0001";
    put(&mut shell, &token, &response(id, "Original title", "list"));
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Keep "},{"type":"file","file_id":id,"text":"https://fixture.invalid/file","opaque":1}
        ]}]}),
    ];
    fixture::mentions::set_blocks(&mut shell, blocks.clone());
    let draft = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (draft.visit, draft.serial);
    let opening = opening::begin(&mut shell, visit, serial, "long", 77).unwrap();
    invalidate(&mut shell, &token.owner.team, id);
    assert!(!opening::finish(&mut shell, opening));
    assert!(Observation::capture(&shell, &token, id).is_err());
    // A partial refresh keeps the actual prior title, rather than starting an
    // invented empty store when invalidation retires the read witness.
    put(
        &mut shell,
        &token,
        &json!({"ok":true,"file":{"id":id,"mode":"list","created":2}}),
    );
    let observed = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(
        observed
            .label(&blocks[0]["elements"][0]["elements"][1])
            .unwrap(),
        "Original title"
    );
    let opening = opening::begin(&mut shell, visit, serial, "long", 78).unwrap();
    assert!(opening::finish(&mut shell, opening));
    let labels = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap()
        .labels
        .clone();
    put(&mut shell, &token, &response(id, "Later title", "list"));
    assert_eq!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .rich
            .as_ref()
            .unwrap()
            .labels,
        labels
    );
    assert!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value("long")
            == Some(&SectionValue::LongText(blocks))
    );
}
#[test]
fn wrong_id_account_and_deleted_file_never_supply_opening_labels() {
    let (mut shell, token) = setup();
    let generation = socket(&shell, &token).unwrap();
    assert!(
        install(
            &mut shell,
            &token,
            "FFILE0001",
            generation,
            0,
            &response("FFILE0002", "Title", "quip")
        )
        .is_err()
    );
    put(&mut shell, &token, &response("FFILE0001", "Title", "quip"));
    let observed = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .deleted_files
        .insert("FFILE0001".into());
    assert!(!observed.current(&shell));
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .deleted_files
        .clear();
    shell.core.account_epoch += 1;
    assert!(!observed.current(&shell));
}

#[test]
fn local_metadata_changes_retire_inflight_reads_before_any_row_exists() {
    let (mut shell, token) = setup();
    let stamp = Arc::new(());
    cache(&mut shell, &token)
        .unwrap()
        .changes
        .insert("FFILE0001".into(), stamp.clone());
    assert!(read_current(&shell, &token, "FFILE0001", &stamp));
    invalidate(&mut shell, &token.owner.team, "FFILE0002");
    assert!(read_current(&shell, &token, "FFILE0001", &stamp));
    // The existing shared hook also protects reads begun before a confirmed
    // local file rename, even when its realtime tracker has no pending read.
    crate::file_events::invalidate(&mut shell, &token.owner.team, "FFILE0001");
    assert!(!read_current(&shell, &token, "FFILE0001", &stamp));
    assert!(
        shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .unwrap()
            .store
            .row("FFILE0001")
            .is_err()
    );
}

#[test]
fn canvas_blocks_require_independent_runtime_observation_and_preserve_literal_title() {
    let (mut shell, token) = setup();
    let mut raw = response("FFILE0001", "Wrong file-title fallback", "quip");
    raw["file"]["title_blocks"] = json!([{"type":"rich_text","opaque":"retained",
    "elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":":wave: &amp; ","style":{"bold":true}},
        {"type":"text","text":"*literal*","opaque":17}
    ]}]}]);
    put(&mut shell, &token, &raw);
    let wire =
        json!({"type":"canvas","file_id":"FFILE0001","text":"wire URL","style":{"italic":true}});
    let opening = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(opening.label(&wire).unwrap(), "&amp; *literal*");

    // A healthy file receipt cannot fill in an unobserved independent store.
    shell.canvas_titles = Default::default();
    assert!(!opening.current(&shell));
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
    let retained = shell
        .profile_edit
        .sections
        .file_titles
        .as_ref()
        .unwrap()
        .store
        .row("FFILE0001")
        .unwrap();
    assert_eq!(
        retained.source()["title_blocks"],
        raw["file"]["title_blocks"]
    );

    // A genuinely new runtime can be observed, but cannot revive an old witness.
    shell.canvas_titles = crate::canvas::quip_titles::State::fresh();
    assert!(!opening.current(&shell));
    let reopened = Observation::capture(&shell, &token, "FFILE0001").unwrap();
    assert_eq!(reopened.label(&wire).unwrap(), "&amp; *literal*");

    let mut renamed = raw.clone();
    renamed["file"]["title_blocks"][0]["elements"][0]["elements"] =
        json!([{"type":"text","text":":memo: Changed &amp;"}]);
    put(&mut shell, &token, &renamed);
    assert!(!reopened.current(&shell));
    assert_eq!(reopened.label(&wire).unwrap(), "&amp; *literal*");
    assert_eq!(
        Observation::capture(&shell, &token, "FFILE0001")
            .unwrap()
            .label(&wire)
            .unwrap(),
        "Changed &amp;"
    );
}

#[test]
fn emoji_title_context_keeps_file_runtime_and_token_custody_separate() {
    let (mut shell, token) = setup();
    let id = "FFILE0001";
    let mut raw = response(id, "Wrong fallback", "quip");
    raw["file"]["title_blocks"] = json!([{"type":"rich_text","block_id":"kept-block","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":"Project "},
        {"type":"emoji","name":"wave","style":{"bold":true},"opaque":{"keep":1}},
        {"type":"text","text":" plan"}]}]}]);
    put(&mut shell, &token, &raw);
    let wire = json!({"type":"canvas","file_id":id,"text":"https://fixture.invalid/unchanged",
        "url":"https://fixture.invalid/unchanged","style":{"italic":true},"opaque":{"keep":42}});
    let original_wire = wire.clone();
    let observed = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(observed.label(&wire).unwrap(), "Project :wave: plan");
    assert!(observed.current(&shell));
    assert_eq!(
        observed.emoji.descriptor()["origin"],
        "application-presentation"
    );
    assert_eq!(wire, original_wire);
    assert_eq!(
        observed.row.source()["title_blocks"],
        raw["file"]["title_blocks"]
    );
    // A changed source retires this observation while the mounted label remains
    // immutable. Emoji-only extraction is empty, never the separate fallback.
    let mut changed = raw.clone();
    changed["file"]["title_blocks"][0]["elements"][0]["elements"] =
        json!([{"type":"emoji","name":"wave"}]);
    put(&mut shell, &token, &changed);
    assert!(!observed.current(&shell));
    assert_eq!(observed.label(&wire).unwrap(), "Project :wave: plan");
    let empty = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(empty.label(&wire).unwrap(), "");
    shell.canvas_titles = Default::default();
    assert!(!empty.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
    shell.canvas_titles = crate::canvas::quip_titles::State::fresh();
    let fresh = Observation::capture(&shell, &token, id).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .rt_generation += 1;
    assert!(!fresh.current(&shell));
    put(&mut shell, &token, &changed);
    let next = Observation::capture(&shell, &token, id).unwrap();
    shell.core.account_epoch += 1;
    assert!(!next.current(&shell));
}

#[test]
fn plain_canvas_and_whole_list_titles_do_not_claim_a_quip_dependency() {
    let (mut shell, token) = setup();
    shell.canvas_titles = Default::default();
    for (id, mode, kind) in [
        ("FFILE0001", "quip", "canvas"),
        ("FFILE0002", "list", "file"),
    ] {
        let mut raw = response(id, "Plain title", mode);
        if mode == "list" {
            raw["file"]["title_blocks"] = json!([{"type":"future"}]);
        }
        put(&mut shell, &token, &raw);
        let observed = Observation::capture(&shell, &token, id).unwrap();
        assert_eq!(
            observed.label(&json!({"type":kind,"file_id":id})).unwrap(),
            "Plain title"
        );
        assert!(observed.quip.is_none());
    }
}

#[test]
fn nested_member_dependency_does_not_invalidate_current_file_receipt() {
    let (mut shell, token) = setup();
    let mut raw = response("FFILE0001", "Not the nested title", "quip");
    raw["file"]["title_blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"With "},{"type":"user","user_id":"UMISSING01"}]}]}]);
    put(&mut shell, &token, &raw);
    assert!(metadata_current(&shell, &token, "FFILE0001"));
    assert!(Observation::capture(&shell, &token, "FFILE0001").is_err());
    assert!(reuse_metadata(&shell, &token, "FFILE0001", false));
    assert!(reuse_metadata(&shell, &token, "FFILE0001", true));
    let before = dependencies(&shell, &token, &["FFILE0001".into()]).unwrap();
    assert_eq!(before.ids, vec!["UMISSING01"]);
    raw["file"]["title_blocks"][0]["elements"][0]["elements"][1]["user_id"] = json!("UREPLACED01");
    put(&mut shell, &token, &raw);
    assert!(!before.current(&shell, &token));
    invalidate(&mut shell, &token.owner.team, "FFILE0001");
    assert!(!reuse_metadata(&shell, &token, "FFILE0001", false));
    assert!(!reuse_metadata(&shell, &token, "FFILE0001", true));
}
#[test]
fn list_metadata_does_not_acquire_canvas_title_users() {
    let mut raw = response("FLIST0001", "List name", "list");
    raw["file"]["title_blocks"] = json!([{"type":"unsupported"}]);
    assert!(title_user_ids(&raw["file"]).unwrap().is_empty());
}

#[test]
fn profile_empty_canvas_uses_explicit_english_without_replacing_wire_or_owner() {
    let (mut shell, token) = setup();
    shell.canvas_titles = Default::default(); // No Quip observation for plain titles.
    let id = "FFILE0001";
    put(&mut shell, &token, &response(id, "Old title", "quip"));
    let old = Observation::capture(&shell, &token, id).unwrap();
    let wire = json!({"type":"canvas","file_id":id,"url":"https://fixture.invalid/original",
        "text":"original wire","style":{"bold":true},"opaque":{"keep":7}});
    let before = wire.clone();
    let mut empty = response(id, " \t\n", "quip");
    empty["file"]["title_blocks"] = json!([]);
    put(&mut shell, &token, &empty);
    assert!(!old.current(&shell));
    assert_eq!(old.label(&wire).unwrap(), "Old title"); // Mounted label remains frozen.
    let current = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(current.label(&wire).unwrap(), "Untitled");
    assert_eq!(wire, before);
    assert_eq!(current.row.source(), &empty["file"]);
    assert!(current.quip.is_none());
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .rt_generation += 1;
    assert!(!current.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
}

#[test]
fn profile_empty_blocks_still_require_actual_quip_observation() {
    let (mut shell, token) = setup();
    let id = "FFILE0001";
    let mut empty = response(id, "not extraction", "quip");
    empty["file"]["title_blocks"] = json!([{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":" \t"}]}]}]);
    put(&mut shell, &token, &empty);
    shell.canvas_titles = Default::default();
    assert!(Observation::capture(&shell, &token, id).is_err());
    shell.canvas_titles = crate::canvas::quip_titles::State::fresh();
    let current = Observation::capture(&shell, &token, id).unwrap();
    assert_eq!(
        current
            .label(&json!({"type":"canvas","file_id":id}))
            .unwrap(),
        "Untitled"
    );
    shell.canvas_titles = crate::canvas::quip_titles::State::fresh();
    assert!(!current.current(&shell));
}
