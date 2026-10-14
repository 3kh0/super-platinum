//! Disconnected normal-input fixture; no credentials, transport or app callback.
use super::*;
use crate::message_buttons::external_fixture as shared;
use serde_json::json;

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-multi-external-select" | "app-modal-multi-external-select-narrow")
        )
}

pub(crate) fn view() -> Value {
    let mut view = shared::view();
    view["hash"] = json!("modal-multi-external-1");
    view["title"]["text"] = json!("Choose releases");
    view["blocks"][0]["optional"] = json!(false);
    view["blocks"][0]["element"] = json!({"type":"multi_external_select","action_id":"pick",
        "min_query_length":3,"max_selected_items":2,
        "initial_options":[shared::option("Installed release","installed")],
        "placeholder":{"type":"plain_text","text":"Find releases"}});
    view["blocks"].as_array_mut().unwrap().insert(
        1,
        json!({"type":"input","block_id":"optional",
        "optional":true,"label":{"type":"plain_text","text":"Optional releases"},
        "element":{"type":"multi_external_select","action_id":"pick","min_query_length":3,
            "initial_options":[shared::option("Ignored initial","ignored")]}}),
    );
    view["state"] = json!({"values":{"optional":{"pick":{"type":"multi_external_select","selected_options":[]}}}});
    view
}

pub(crate) fn install(shell: &mut ShellState) {
    shared::install(shell);
    shell.core.session = None;
    shell.core.transport = None;
    remount(shell);
}

/// Explicitly starts a fresh synthetic modal after the native Cancel path.
pub(super) fn remount(shell: &mut ShellState) {
    shell.app_modal = Some(Session::fixture(view()));
    shell.overlay = Some(Overlay::AppModal);
}

pub(super) fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "pick".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_input_fixture_is_supported_and_saved_empty_overrides_initial() {
        let form = super_platinum_core::block_actions::form::ModalForm::parse(&view()).unwrap();
        assert!(form.unsupported.is_empty());
        let wire = form.submission().unwrap();
        assert_eq!(
            wire["values"]["choice"]["pick"]["selected_options"],
            json!([shared::option("Installed release", "installed")])
        );
        assert_eq!(
            wire["values"]["optional"]["pick"]["selected_options"],
            json!([])
        );
        assert_eq!(
            wire["values"]["note"]["value"]["value"],
            "Keep this sibling"
        );
    }
}
