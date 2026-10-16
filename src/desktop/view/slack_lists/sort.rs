use super::*;
use crate::slack_lists::rename::sort;
use super_platinum_core::slack_lists::ViewSortChoice;
pub(super) fn action(state: Signal<ShellState>, visit: u64) -> Element {
    let selection = crate::slack_lists::rename::selection(&state.read()).unwrap_or_default();
    let disabled = !sort::can_open(&state.read());
    rsx! {button{id:"slack-list-sort",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,title:"Save one Number sort for this shared Table",onclick:move |_|sort::open(state,visit,selection),{icon(Icon::Sort,"icon")}span{"Sort"}}}
}
fn label(choice: Option<&ViewSortChoice>, choices: &[(String, String)]) -> String {
    match choice {
        None => "Unavailable".into(),
        Some(ViewSortChoice::None) => "No sort".into(),
        Some(ViewSortChoice::Number {
            column_id,
            ascending,
        }) => format!(
            "{}: {}",
            choices
                .iter()
                .find(|(id, _)| id == column_id)
                .map(|(_, label)| label.as_str())
                .unwrap_or("Number field"),
            if *ascending {
                "Ascending"
            } else {
                "Descending"
            }
        ),
    }
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: sort::Presentation,
    editable: bool,
) -> Element {
    let choice = p.choice;
    let field = match &p.desired {
        ViewSortChoice::None => String::new(),
        ViewSortChoice::Number { column_id, .. } => column_id.clone(),
    };
    let ascending = match &p.desired {
        ViewSortChoice::Number { ascending, .. } => *ascending,
        ViewSortChoice::None => true,
    };
    let direction = if ascending { "ascending" } else { "descending" };
    let rendered_field = p.desired.clone();
    let rendered_direction = p.desired.clone();
    let direction_column = field.clone();
    let original = label(Some(&p.original), &p.choices);
    let current = label(p.current.as_ref(), &p.choices);
    rsx! {
        label{r#for:"slack-list-sort-field","Number field"}
        select{id:"slack-list-sort-field",value:"{field}",disabled:!editable,"data-sort-choice":"{choice}",onchange:move |e|{
            let id=e.value();let desired=if id.is_empty(){ViewSortChoice::None}else{ViewSortChoice::Number{column_id:id,ascending}};
            sort::input(state,serial,visit,choice,rendered_field.clone(),desired);
        },option{value:"",selected:field.is_empty(),"No sort"}for (id,name) in p.choices{option{value:"{id}",selected:field==id,"{name}"}}}
        label{r#for:"slack-list-sort-direction","Direction"}
        select{id:"slack-list-sort-direction",value:direction,disabled:!editable||field.is_empty(),onchange:move |e|{
            if let Some(ascending)=parse_direction(&e.value()) && !direction_column.is_empty(){sort::input(state,serial,visit,choice,rendered_direction.clone(),ViewSortChoice::Number{column_id:direction_column.clone(),ascending});}
        },option{value:"ascending",selected:ascending,"Ascending"}option{value:"descending",selected:!ascending,"Descending"}}
        p{id:"slack-list-sort-current",class:"slack-list-editor-hint","Original: {original}. Current: {current}."}
        p{class:"slack-list-editor-hint","Save changes this shared Table for everyone with access. Only currently loaded items can be ordered here; stored values are kept. Choose No sort to remove this rule."}
    }
}
fn parse_direction(value: &str) -> Option<bool> {
    match value {
        "ascending" => Some(true),
        "descending" => Some(false),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direction_values_are_exact() {
        assert_eq!(parse_direction("ascending"), Some(true));
        assert_eq!(parse_direction("descending"), Some(false));
        for value in ["true", "false", "Ascending", "descending ", ""] {
            assert_eq!(parse_direction(value), None);
        }
    }
}
