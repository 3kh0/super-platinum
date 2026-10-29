use super::*;
use crate::app_modal::scalar::{Kind, Owner};

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
    kind: Kind,
) -> Element {
    // Event closures own only Weak. This mounted hook alone keeps the binding
    // alive; captured kind also prevents an email/URL replacement retarget.
    let binding = use_hook(|| std::rc::Rc::new(()));
    let owner = use_hook(|| match kind {
        Kind::Email => crate::app_modal::email::Owner::capture(&state.read(), &input_key, &binding),
        Kind::Url => Owner::capture_url(&state.read(), &input_key, &binding),
        Kind::Integer => Owner::capture_integer(&state.read(), &input_key, &binding),
    });
    let disabled = locked
        || owner
            .as_ref()
            .is_none_or(|owner| !owner.current(&state.read()));
    let (input_type, symbol) = match kind {
        Kind::Email => ("email", Some(Icon::Mail)),
        Kind::Url => ("url", Some(Icon::Link)),
        Kind::Integer => ("number", None),
    };
    // Slack's URL validator accepts schemeless literals. Only URL is detached
    // from HTML form validation; every existing control keeps its association.
    // Typed submission still applies all required/URL/currentness guards.
    // No synthetic Enter submission: native key/form behavior stays unaccepted.
    let form = (kind == Kind::Url).then_some("");
    let class = symbol.map(|_| "home-email-control");
    let native_required = (kind == Kind::Integer).then_some(true);
    rsx! {
        span {class,
            if let Some(symbol)=symbol {{icon(symbol,"home-email-icon")}}
            input {id:"{id}",name:"{id}",r#type:input_type,form,required:native_required,initial_value:"{initial}",
                placeholder:"{placeholder}",autocorrect:"off",autocapitalize:"off",disabled,
                "aria-labelledby":"{label}","aria-describedby":"{description}",
                "aria-required":"{required}","aria-invalid":"{invalid}",
                oninput:move |event|if let Some(owner)=owner.as_ref(){owner.set(state,event.value());}}
        }
    }
}
