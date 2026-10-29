//! Bounded required integer quantity fixtures; no request reaches Slack.
use super::*;

pub(crate) mod fixture {
    use super::*;
    use serde_json::json;

    pub(crate) fn view() -> Value {
        json!({"id":"V_MODAL_QUANTITY","app_id":"A_FIXTURE","type":"modal",
            "title":{"type":"plain_text","text":"Order quantities"},
            "submit":{"type":"plain_text","text":"Save quantities"},"blocks":[
            {"type":"input","block_id":"quantity","label":{"type":"plain_text","text":"Quantity"},
                "hint":{"type":"plain_text","text":"Enter a whole quantity."},
                "element":{"type":"number_input","action_id":"value","is_decimal_allowed":false,"initial_value":"1"}},
            {"type":"input","block_id":"reserve","label":{"type":"plain_text","text":"Reserve"},
                "element":{"type":"number_input","action_id":"value","is_decimal_allowed":false,"initial_value":"0"}},
            {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Order note"},
                "element":{"type":"plain_text_input","action_id":"value","initial_value":"Order"}}
        ]})
    }

    pub(crate) fn install(shell: &mut ShellState) {
        shell.core.transport = None;
        shell.toast = None;
        shell.app_modal = Some(Session::fixture(view()));
        shell.overlay = Some(Overlay::AppModal);
    }

    pub(crate) fn install_signed(shell: &mut ShellState) {
        let mut v = view();
        v["title"]["text"] = json!("Inventory adjustments");
        v["blocks"][0]["label"]["text"] = json!("Adjustment");
        v["blocks"][0]["element"]["initial_value"] = json!("-1");
        shell.core.transport = None;
        shell.toast = None;
        shell.app_modal = Some(Session::fixture(v));
        shell.overlay = Some(Overlay::AppModal);
    }
}

#[cfg(test)]
mod signed_tests;
#[cfg(test)]
mod tests;
