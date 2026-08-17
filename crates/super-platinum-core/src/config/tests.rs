use std::sync::MutexGuard;

use super::*;

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("config test lock poisoned")
}

fn reset() {
    let _ = std::fs::remove_dir_all(config_dir().expect("config dir"));
    test_secrets()
        .lock()
        .expect("test secret store poisoned")
        .clear();
}

fn session() -> Session {
    Session {
        d_cookie: "xoxd-cookie".into(),
        workspaces: BTreeMap::from([
            (
                "T_ONE".into(),
                WorkspaceSession {
                    team_id: "T_ONE".into(),
                    enterprise_id: None,
                    user_id: "U_ONE".into(),
                    name: "One".into(),
                    url: "https://one.slack.com".into(),
                    token: "xoxc-one".into(),
                },
            ),
            (
                "T_TWO".into(),
                WorkspaceSession {
                    team_id: "T_TWO".into(),
                    enterprise_id: Some("E_TWO".into()),
                    user_id: "U_TWO".into(),
                    name: "Two".into(),
                    url: "https://two.slack.com".into(),
                    token: "xoxc-two".into(),
                },
            ),
        ]),
    }
}

#[test]
fn appearance_settings_roundtrip_with_managed_background() {
    let _guard = test_lock();
    reset();
    let settings = Settings {
        preset: ThemePreset::PaperBag,
        colors: RoleColorOverrides {
            primary: Some(HexColor::from_rgb(0x12, 0x34, 0x56)),
            danger: Some(HexColor::from_rgb(0xAA, 0x22, 0x33)),
            ..RoleColorOverrides::default()
        },
        background: Some(BackgroundSettings {
            file_name: "fixture.webp".to_owned(),
            fit: BackgroundFit::Contain,
            dim: 0.32,
            surface_opacity: 0.79,
        }),
        gap: 11.0,
        ..Settings::default()
    };

    save_settings(&settings).expect("save appearance");
    let loaded = load_settings();

    assert_eq!(loaded, settings);
    let serialized =
        std::fs::read_to_string(settings_path().expect("settings path")).expect("settings");
    assert!(serialized.contains("\"preset\": \"paper_bag\""));
    assert!(serialized.contains("\"primary\": \"#123456\""));
    assert!(!serialized.contains("\"accent\""));
}

#[test]
fn legacy_accent_migrates_to_role_overrides() {
    let _guard = test_lock();
    reset();
    std::fs::create_dir_all(config_dir().expect("config dir")).expect("create config dir");
    std::fs::write(
            settings_path().expect("settings path"),
            br#"{"accent":"purple","gap":12,"panel_radius":8,"border_thickness":1,"sidebar_width":240}"#,
        )
        .expect("write legacy settings");

    let settings = load_settings();

    assert_eq!(settings.preset, ThemePreset::Countertop);
    assert_eq!(
        settings.colors.primary.expect("migrated primary").as_hex(),
        "#B382DA"
    );
    assert_eq!(
        settings.colors.hover.expect("migrated hover").as_hex(),
        "#C9A4E6"
    );
}

#[test]
fn colors_require_six_digit_hex_values() {
    assert_eq!(
        HexColor::try_from("#e8875b".to_owned())
            .expect("valid color")
            .as_hex(),
        "#E8875B"
    );
    assert!(HexColor::try_from("#fff".to_owned()).is_err());
    assert!(HexColor::try_from("not-a-color".to_owned()).is_err());
}

#[test]
fn managed_background_paths_reject_traversal() {
    let safe = BackgroundSettings {
        file_name: "background.png".to_owned(),
        fit: BackgroundFit::Cover,
        dim: 0.45,
        surface_opacity: 0.88,
    };
    assert!(
        background_path(&safe)
            .expect("safe path")
            .ends_with("backgrounds/background.png")
    );
    assert!(
        background_path(&BackgroundSettings {
            file_name: "../outside.png".to_owned(),
            ..safe
        })
        .is_none()
    );
}

#[test]
fn roundtrips_session_with_single_secret_entry() {
    let _guard = test_lock();
    reset();

    save_session(&session()).expect("save session");

    let metadata =
        std::fs::read_to_string(session_path().expect("session path")).expect("read metadata");
    assert!(!metadata.contains("xoxd-cookie"));
    assert!(!metadata.contains("xoxc-one"));

    let secrets = test_secrets().lock().expect("test secret store poisoned");
    assert!(secrets.contains_key(secret_account()));
    assert!(!secrets.contains_key("d_cookie"));
    assert!(!secrets.contains_key(&token_account("T_ONE")));
    drop(secrets);

    let loaded = load_session().expect("load session").expect("session");
    assert_eq!(loaded.d_cookie, "xoxd-cookie");
    assert_eq!(loaded.workspaces["T_ONE"].token, "xoxc-one");
    assert_eq!(loaded.workspaces["T_TWO"].token, "xoxc-two");
}

#[test]
fn saves_switches_and_removes_multiple_accounts() {
    let _guard = test_lock();
    reset();

    let first = session();
    save_session(&first).expect("save first account");
    let first_id = load_accounts()
        .expect("load accounts")
        .expect("accounts")
        .active_account;

    let mut second = session();
    second.d_cookie = "xoxd-second".into();
    second.workspaces.get_mut("T_ONE").unwrap().user_id = "U_OTHER".into();
    second.workspaces.remove("T_TWO");
    save_session(&second).expect("save second account");

    let accounts = load_accounts().expect("load accounts").expect("accounts");
    assert_eq!(accounts.sessions.len(), 2);
    assert_ne!(accounts.active_account, first_id);
    assert_eq!(
        accounts.sessions[&accounts.active_account].d_cookie,
        "xoxd-second"
    );

    set_active_account(&first_id).expect("switch account");
    assert_eq!(
        load_session().expect("load session").unwrap().d_cookie,
        "xoxd-cookie"
    );

    let remaining = remove_account(&first_id)
        .expect("remove account")
        .expect("remaining account");
    let accounts = load_accounts().expect("load accounts").expect("accounts");
    assert_eq!(accounts.sessions.len(), 1);
    assert_eq!(accounts.active_account, remaining);
    assert_eq!(accounts.sessions[&remaining].d_cookie, "xoxd-second");
}

#[test]
fn signing_in_again_refreshes_matching_account() {
    let _guard = test_lock();
    reset();

    let first = session();
    save_session(&first).expect("save account");
    let mut refreshed = first;
    refreshed.d_cookie = "xoxd-refreshed".into();
    refreshed.workspaces.get_mut("T_ONE").unwrap().token = "xoxc-refreshed".into();
    save_session(&refreshed).expect("refresh account");

    let accounts = load_accounts().expect("load accounts").expect("accounts");
    assert_eq!(accounts.sessions.len(), 1);
    let active = &accounts.sessions[&accounts.active_account];
    assert_eq!(active.d_cookie, "xoxd-refreshed");
    assert_eq!(active.workspaces["T_ONE"].token, "xoxc-refreshed");
}

#[test]
fn sign_in_recovers_from_corrupt_session_metadata() {
    let _guard = test_lock();
    reset();
    std::fs::create_dir_all(config_dir().expect("config dir")).expect("create config dir");
    std::fs::write(session_path().expect("session path"), b"{").expect("write corrupt session");

    save_session(&session()).expect("replace corrupt session");

    let loaded = load_session().expect("load session").expect("session");
    assert_eq!(loaded.d_cookie, "xoxd-cookie");
    assert_eq!(loaded.workspaces["T_ONE"].token, "xoxc-one");
}

#[test]
fn migrates_previous_single_session_format() {
    let _guard = test_lock();
    reset();

    let session = session();
    let meta = SessionMeta::from(&session);
    std::fs::create_dir_all(config_dir().expect("config dir")).expect("create config dir");
    std::fs::write(
        session_path().expect("session path"),
        serde_json::to_string_pretty(&meta).expect("serialize metadata"),
    )
    .expect("write metadata");
    set_secret(
        legacy_secret_account(),
        &serde_json::to_string(&SessionSecrets::from(&session)).expect("serialize secrets"),
    )
    .expect("save legacy secrets");

    let accounts = load_accounts().expect("load accounts").expect("accounts");

    assert_eq!(accounts.sessions.len(), 1);
    assert_eq!(
        accounts.sessions[&accounts.active_account].workspaces["T_ONE"].token,
        "xoxc-one"
    );
    let secrets = test_secrets().lock().expect("test secret store poisoned");
    assert!(secrets.contains_key(secret_account()));
    assert!(!secrets.contains_key(legacy_secret_account()));
}

#[test]
fn migrates_legacy_secrets_to_single_entry() {
    let _guard = test_lock();
    reset();

    let session = session();
    let meta = SessionMeta {
        workspaces: session
            .workspaces
            .values()
            .map(WorkspaceMeta::from)
            .collect(),
    };
    std::fs::create_dir_all(config_dir().expect("config dir")).expect("create config dir");
    std::fs::write(
        session_path().expect("session path"),
        serde_json::to_string_pretty(&meta).expect("serialize metadata"),
    )
    .expect("write metadata");
    set_secret("d_cookie", &session.d_cookie).expect("set d cookie");
    set_secret(&token_account("T_ONE"), "xoxc-one").expect("set token");
    set_secret(&token_account("T_TWO"), "xoxc-two").expect("set token");

    let loaded = load_session().expect("load session").expect("session");

    assert_eq!(loaded.workspaces.len(), 2);
    assert_eq!(loaded.workspaces["T_ONE"].token, "xoxc-one");
    assert!(
        test_secrets()
            .lock()
            .expect("test secret store poisoned")
            .contains_key(secret_account())
    );
    assert!(
        !test_secrets()
            .lock()
            .expect("test secret store poisoned")
            .contains_key("d_cookie")
    );
}
