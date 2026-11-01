//! Closed normal/narrow layout fixture; never a live Slack transport fallback.
use super::*;
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-url-input" | "app-home-url-input-narrow")
        )
        && std::env::var("SUPER_PLATINUM_APP_HOME_URL_CHECK").as_deref() == Ok("1")
}
pub(crate) fn install(s: &mut ShellState) {
    super::super::fixture::install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"header","text":{"type":"plain_text","text":"Project details"}},
        {"type":"input","block_id":"website","dispatch_action":true,"label":{"type":"plain_text","text":"Website"},"element":{"type":"url_text_input","action_id":"value","initial_value":"https://example.test/project","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed","on_character_entered"]}}},
        {"type":"input","block_id":"reference","optional":true,"label":{"type":"plain_text","text":"Reference URL"},"element":{"type":"url_text_input","action_id":"value","placeholder":{"type":"plain_text","text":"Enter a URL"}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Project website"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(s);
}
