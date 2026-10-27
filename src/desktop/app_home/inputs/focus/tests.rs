//! Logical lifecycle only; actual focus/scroll/selection requires native evidence.
use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "enter".into(),
        action_id: "value".into(),
    }
}
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    text::fixture::install(&mut s);
    assert!(config(&s, &key()).is_some());
    s
}
fn refresh(s: &mut ShellState, change: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    change(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::hydrate(s);
}
#[test]
fn source_flag_and_unrelated_refresh_keep_logical_identity() {
    let mut s = setup();
    let initial = config(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["focus_on_load"] = json!(true)
    });
    let changed = config(&s, &key()).unwrap();
    assert_eq!(changed["instance"], initial["instance"]);
    assert_eq!(changed["flag"], true);
    assert_ne!(changed["revision"], initial["revision"]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["label"]["text"] = json!("Changed label")
    });
    assert_eq!(config(&s, &key()).unwrap()["instance"], initial["instance"]);
}
#[test]
fn explicit_value_reset_boundary_preserves_focus_identity() {
    let mut s = setup();
    let before = config(&s, &key()).unwrap();
    let o = owner(&s).unwrap();
    let raw = form(&s).unwrap().raw_view().clone();
    s.app_home.forms.insert(o, HomeForm::parse(&raw).unwrap());
    s.app_home.surface.as_mut().unwrap().revision = next();
    hydrate(&mut s);
    assert_eq!(config(&s, &key()).unwrap()["instance"], before["instance"]);
}
#[test]
fn new_visit_removal_and_kind_change_each_rearm() {
    let mut s = setup();
    let first = config(&s, &key()).unwrap()["instance"].clone();
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    let second = config(&s, &key()).unwrap()["instance"].clone();
    assert_ne!(first, second);
    let original = form(&s).unwrap().raw_view().clone();
    refresh(&mut s, |raw| {
        raw["blocks"].as_array_mut().unwrap().remove(0);
    });
    assert!(config(&s, &key()).is_none());
    refresh(&mut s, |raw| *raw = original);
    let third = config(&s, &key()).unwrap()["instance"].clone();
    assert_ne!(second, third);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["multiline"] = json!(true)
    });
    let fourth = config(&s, &key()).unwrap();
    assert_ne!(third, fourth["instance"]);
    assert_eq!(fourth["kind"], "multiline");
}
#[test]
fn quiet_fields_have_focus_identity_without_native_dispatch_entries() {
    let s = setup();
    let quiet = InputKey {
        block_id: "quiet".into(),
        action_id: "value".into(),
    };
    assert_eq!(config(&s, &quiet).unwrap()["kind"], "singleline");
    assert!(!text::dispatching(&s, &quiet));
    assert_eq!(
        form(&s).unwrap().fields[&quiet].value,
        InputValue::Text("Sibling".into())
    );
}
#[test]
fn stale_context_cannot_publish_focus_configuration() {
    let mut s = setup();
    s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(config(&s, &key()).is_none());
    assert!(
        proof(&s)["focusWidgets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w["current"] == false)
    );
}

#[test]
fn date_reuses_logical_focus_history_without_creating_text_dispatch_mount() {
    let mut s = setup();
    let key = InputKey {
        block_id: "day".into(),
        action_id: "value".into(),
    };
    let first = config(&s, &key).unwrap();
    assert_eq!(first["kind"], "date");
    refresh(&mut s, |raw| {
        let rows = raw["blocks"].as_array_mut().unwrap();
        let row = rows.iter_mut().find(|r| r["block_id"] == "day").unwrap();
        row["element"]["focus_on_load"] = json!(true);
    });
    let second = config(&s, &key).unwrap();
    assert_eq!(first["instance"], second["instance"]);
    assert_eq!(second["flag"], true);
    assert!(!text::dispatching(&s, &key));
    let o = owner(&s).unwrap();
    let raw = form(&s).unwrap().raw_view().clone();
    s.app_home.forms.insert(o, HomeForm::parse(&raw).unwrap());
    hydrate(&mut s);
    assert_eq!(config(&s, &key).unwrap()["instance"], first["instance"]);
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    assert_ne!(config(&s, &key).unwrap()["instance"], first["instance"]);
}
