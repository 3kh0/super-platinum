use super::*;

#[component]
pub(super) fn Control(
    state: Signal<ShellState>,
    input_key: InputKey,
    initial: String,
    id: String,
    label: String,
    description: String,
    required: String,
    invalid: String,
    placeholder: String,
    locked: bool,
) -> Element {
    rsx! {
        super::scalar::Control {state,input_key,initial,id,label,description,required,invalid,
            placeholder,locked,kind:crate::app_modal::scalar::Kind::Email}
    }
}
