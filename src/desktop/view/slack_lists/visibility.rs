use super::*;
use crate::slack_lists::rename::visibility;
pub(super) fn action(state: Signal<ShellState>, _visit: u64) -> Element {
    let stamp = crate::slack_lists::refinement::toolbar::stamp(&state.read());
    let disabled = !crate::slack_lists::rename::local_refinement::can_open(
        &state.read(),
        crate::slack_lists::rename::local_refinement::Kind::Visibility,
    );
    rsx! {button{id:"slack-list-fields",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,
    title:"Show or hide fields in your current view",onclick:move |_|crate::slack_lists::rename::local_refinement::open_stamped(state,crate::slack_lists::rename::local_refinement::Kind::Visibility,stamp.clone()),
    {icon(Icon::Settings,"icon")}span{"Fields"}}}
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: visibility::Presentation,
    editable: bool,
    local: bool,
) -> Element {
    let choice = p.choice;
    let chosen_column = p.column.clone();
    let input_column = p.column.clone();
    let current = p
        .current
        .map(|v| if v { "Shown" } else { "Hidden" })
        .unwrap_or("Unavailable");
    let original = if p.original { "Shown" } else { "Hidden" };
    let value = if p.desired { "shown" } else { "hidden" };
    rsx! {
        label{r#for:"slack-list-field-choice","Field"}
        select{id:"slack-list-field-choice",value:"{p.column}",disabled:!editable,"data-visibility-choice":"{choice}",
            onchange:move |e|visibility::choose(state,serial,visit,choice,chosen_column.clone(),e.value()),
            for (id,label) in p.choices {option{value:"{id}",selected:p.column==id,"{label}"}}
        }
        label{r#for:"slack-list-field-visible","Visibility"}
        select{id:"slack-list-field-visible",value,disabled:!editable,
            onchange:move |e|{if let Some(value)=parse(&e.value()){visibility::input(state,serial,visit,choice,input_column.clone(),value);}},
            option{value:"shown",selected:p.desired,"Shown"}option{value:"hidden",selected:!p.desired,"Hidden"}
        }
        p{id:"slack-list-field-visibility-current",class:"slack-list-editor-hint","Original: {original}. Current: {current}."}
        p{class:"slack-list-editor-hint",if local {"Apply changes your current view. Use Save for everyone afterward to share it."} else {"Save changes this shared Table for everyone with access. Fields and stored values are kept. Choose a different field to discard the current unsaved choice."}}
    }
}
fn parse(value: &str) -> Option<bool> {
    match value {
        "shown" => Some(true),
        "hidden" => Some(false),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_dom_values_are_not_truthy_coercions() {
        assert_eq!(parse("shown"), Some(true));
        assert_eq!(parse("hidden"), Some(false));
        for value in ["true", "false", "0", "1", "Shown", " hidden", ""] {
            assert_eq!(parse(value), None);
        }
    }
}
