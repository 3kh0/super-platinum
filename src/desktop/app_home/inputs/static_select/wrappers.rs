//! Home actions/accessory placement, without borrowing message authority.
use super::*;
pub(crate) mod fixture;
use super_platinum_core::block_actions::{BlockElementMetadata, BlockElementSource};
#[cfg(test)]
mod tests;

pub(crate) fn inline_key(s: &ShellState, metadata: &BlockElementMetadata) -> Option<InputKey> {
    if !matches!(&metadata.source,BlockElementSource::UnsupportedControl{kind} if kind=="static_select")
    {
        return None;
    }
    let key = InputKey {
        block_id: metadata.block_id.clone()?,
        action_id: metadata.action_id.clone()?,
    };
    source_present(s, &key).then_some(key)
}
fn source_block<'a>(s: &'a ShellState, key: &InputKey) -> Option<&'a Value> {
    s.app_home.surface.as_ref()?.view.as_ref()?.raw()["blocks"]
        .as_array()?
        .iter()
        .find(|block| block["block_id"] == key.block_id)
}
fn source_present(s: &ShellState, key: &InputKey) -> bool {
    let Some(field) = form(s).and_then(|f| f.static_field(key).ok()) else {
        return false;
    };
    let Some(block) = source_block(s, key) else {
        return false;
    };
    let element = match block["type"].as_str() {
        Some("actions") => block["elements"]
            .as_array()
            .and_then(|elements| elements.iter().find(|e| e["action_id"] == key.action_id)),
        Some("section") => block.get("accessory"),
        _ => None,
    };
    element == Some(&field.raw)
}
/// Only source-present admitted inline fields count as rendered. The existing
/// dirty-field fallback still protects values whose element was removed.
pub(crate) fn shown_keys(s: &ShellState, block: &Value) -> Vec<InputKey> {
    if !matches!(block["type"].as_str(), Some("actions" | "section")) {
        return vec![];
    }
    form(s)
        .map(|f| {
            f.fields
                .keys()
                .filter(|k| {
                    block["block_id"] == k.block_id
                        && source_block(s, k) == Some(block)
                        && source_present(s, k)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn render(
    state: Signal<ShellState>,
    visit: actions::Key,
    metadata: &BlockElementMetadata,
) -> Option<Element> {
    let s = state.read();
    let key = inline_key(&s, metadata)?;
    let field = form(&s)?.static_field(&key).ok()?;
    let revision = field.revision;
    let id = crate::view::app_modal::field_id(&key);
    drop(s);
    Some(
        rsx! {Inline{key:"{visit.visit}-{visit.revision}-{id}-{revision}",state,visit,input_key:key}},
    )
}
#[component]
fn Inline(state: Signal<ShellState>, visit: actions::Key, input_key: InputKey) -> Element {
    let s = state.read();
    let Some(field) = form(&s)
        .and_then(|f| f.static_field(&input_key).ok())
        .cloned()
    else {
        return rsx! {};
    };
    let locked = !super::super::current(&s, visit)
        || !super::super::editable(&s)
        || s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| cooling(&s, &h.owner.ctx));
    let status = if loading(&s, &input_key.block_id) {
        Some("Sending app action...".to_owned())
    } else {
        status(&s, &input_key.block_id)
    };
    let id = crate::view::app_modal::field_id(&input_key);
    let description = format!("{id}-inline-status");
    let label = field.raw["accessibility_label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or(field.placeholder.clone())
        .unwrap_or(field.label);
    drop(s);
    rsx! {div{class:"home-static-inline","data-home-static-inline":"true","data-block-id":input_key.block_id.clone(),"data-action-id":input_key.action_id.clone(),
        Control{state,visit,input_key:input_key.clone(),id,label:String::new(),description:description.clone(),required:"false".to_owned(),invalid:"false".to_owned(),placeholder:field.placeholder.unwrap_or_default(),locked,compact:true,accessible_label:label}
        if let Some(status)=status {p{id:description,class:"app-home-notice","data-home-static-status":input_key.block_id.clone(),role:"status","{status}"}}
    }}
}

/// The existing per-block Set is shared by actual Button and select invocations.
/// It is deliberately not a count of outstanding requests.
pub(crate) fn button_started(
    s: &mut ShellState,
    button: &super_platinum_core::block_actions::home::Button,
    route: &context::Witness,
) -> Option<Binding> {
    let block = button.key().0;
    let key = form(s)?
        .fields
        .keys()
        .find(|k| k.block_id == block && dispatching(s, k))
        .cloned()?;
    let h = s.app_home.surface.as_ref()?;
    let binding = Binding {
        owner: h.owner.clone(),
        form: owner(s)?,
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key,
        raw: form(s)?.raw_view().clone(),
    };
    s.app_home.static_select.activity.insert(
        (binding.form.clone(), block.to_owned()),
        Activity {
            binding: binding.clone(),
            route: route.clone(),
            loading: true,
            error: None,
        },
    );
    Some(binding)
}
pub(crate) fn button_finished(
    s: &mut ShellState,
    binding: Binding,
    route: &context::Witness,
    error: Option<String>,
) {
    if !binding.current(s) || !route.current(s) {
        return;
    }
    s.app_home.static_select.activity.insert(
        (binding.form.clone(), binding.key.block_id.clone()),
        Activity {
            binding,
            route: route.clone(),
            loading: false,
            error,
        },
    );
}
