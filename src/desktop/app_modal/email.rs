//! Email facade and disconnected fixtures; shared bindings preserve the email-only entry point.
pub(crate) use super::scalar::Owner;
use super::*;

pub(crate) mod fixture {
    use super::*;
    use serde_json::json;
    pub(crate) fn view() -> Value {
        json!({"id":"V_MODAL_EMAIL","app_id":"A_FIXTURE","type":"modal",
        "title":{"type":"plain_text","text":"Contact details"},
        "submit":{"type":"plain_text","text":"Save details"},
        "blocks":[
            {"type":"input","block_id":"work_email","label":{"type":"plain_text","text":"Work email"},
            "hint":{"type":"plain_text","text":"Use your work email address."},
            "element":{"type":"email_text_input","action_id":"value"}},
            {"type":"input","block_id":"alternate_email","optional":true,
            "label":{"type":"plain_text","text":"Alternate email"},
            "element":{"type":"email_text_input","action_id":"value","initial_value":""}}
        ]})
    }
    pub(crate) fn install(shell: &mut ShellState, invalid: bool) {
        shell.core.transport = None;
        shell.toast = None;
        let mut s = Session::fixture(view());
        if invalid {
            let _ = s.current_form_mut().unwrap().set_value(
                &InputKey {
                    block_id: "work_email".into(),
                    action_id: "value".into(),
                },
                InputValue::Email(Some("unfinished@".into())),
            );
        }
        shell.app_modal = Some(s);
        shell.overlay = Some(Overlay::AppModal);
    }
}

#[cfg(test)]
mod tests;
