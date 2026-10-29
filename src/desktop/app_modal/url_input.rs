//! Submit-only URL field fixtures. No fixture response reaches a Slack transport.
use super::*;

pub(crate) mod fixture {
    use super::*;
    use serde_json::json;
    pub(crate) fn view() -> Value {
        json!({"id":"V_MODAL_URL","app_id":"A_FIXTURE","type":"modal",
            "title":{"type":"plain_text","text":"Project links"},
            "submit":{"type":"plain_text","text":"Save links"},"blocks":[
            {"type":"input","block_id":"project_url","label":{"type":"plain_text","text":"Project URL"},
                "hint":{"type":"plain_text","text":"Enter the link to your project."},
                "element":{"type":"url_text_input","action_id":"value"}},
            {"type":"input","block_id":"backup_url","optional":true,"label":{"type":"plain_text","text":"Another URL"},
                "element":{"type":"url_text_input","action_id":"value","initial_value":""}},
            {"type":"input","block_id":"contact","optional":true,"label":{"type":"plain_text","text":"Contact email"},
                "element":{"type":"email_text_input","action_id":"value"}},
            {"type":"input","block_id":"name","optional":true,"label":{"type":"plain_text","text":"Project name"},
                "element":{"type":"plain_text_input","action_id":"value","initial_value":"Project"}}
        ]})
    }
    pub(crate) fn install(shell: &mut ShellState, invalid: bool) {
        shell.core.transport = None;
        shell.toast = None;
        let mut session = Session::fixture(view());
        if invalid {
            let _ = session.current_form_mut().unwrap().set_value(
                &InputKey {
                    block_id: "project_url".into(),
                    action_id: "value".into(),
                },
                InputValue::Url(Some("not a url".into())),
            );
        }
        shell.app_modal = Some(session);
        shell.overlay = Some(Overlay::AppModal);
    }
    pub(crate) const PREFILL: &str = "https://EXAMPLE.com/Project?q=one%20two#section";
    pub(crate) fn install_prefill(shell: &mut ShellState) {
        install(shell, false);
        let mut view = view();
        view["blocks"][0]["element"]["initial_value"] = json!(PREFILL);
        shell.app_modal = Some(Session::fixture(view));
    }
}

#[cfg(test)]
mod prefill_tests;
#[cfg(test)]
mod tests;
