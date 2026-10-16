use super::*;
use crate::slack_lists::rename::visibility;
pub(super) fn action(state: Signal<ShellState>, visit: u64) -> Element {
    let selection = crate::slack_lists::rename::selection(&state.read()).unwrap_or_default();
    let disabled = !visibility::can_open(&state.read());
    rsx! {button{id:"slack-list-fields",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,
    title:"Show or hide a supported field in this saved Table",onclick:move |_|visibility::open(state,visit,selection),
    {icon(Icon::Settings,"icon")}span{"Fields"}}}
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: visibility::Presentation,
    editable: bool,
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
        p{class:"slack-list-editor-hint","Save changes this shared Table for everyone with access. Fields and stored values are kept. Choose a different field to discard the current unsaved choice."}
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
