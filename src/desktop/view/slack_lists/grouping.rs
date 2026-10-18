use super::*;
use crate::slack_lists::rename::grouping;
use super_platinum_core::slack_lists::ViewGroupingChoice;

pub(super) fn action(state: Signal<ShellState>, _visit: u64) -> Element {
    let shell = state.read();
    let stamp = crate::slack_lists::refinement::toolbar::stamp(&shell);
    let selected = crate::slack_lists::rename::selected_view(&shell)
        .unwrap_or_default()
        .to_owned();
    let disabled = !crate::slack_lists::rename::local_refinement::can_open(
        &shell,
        crate::slack_lists::rename::local_refinement::Kind::Grouping,
    );
    rsx! {button{id:"slack-list-grouping",r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,"data-grouping-view":"{selected}",title:"Group your current view",onclick:move |_|crate::slack_lists::rename::local_refinement::open_stamped(state,crate::slack_lists::rename::local_refinement::Kind::Grouping,stamp.clone()),"Group by"}}
}
fn label(choice: Option<&ViewGroupingChoice>, choices: &[(String, String)]) -> String {
    match choice {
        None => "Unavailable".into(),
        Some(ViewGroupingChoice::None) => "None".into(),
        Some(ViewGroupingChoice::Column { column_id }) => choices
            .iter()
            .find(|(id, _)| id == column_id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Unavailable field".into()),
    }
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: grouping::Presentation,
    editable: bool,
    local: bool,
) -> Element {
    let choice = p.choice;
    let field = match &p.desired {
        ViewGroupingChoice::None => String::new(),
        ViewGroupingChoice::Column { column_id } => column_id.clone(),
    };
    let rendered = p.desired.clone();
    let original = label(Some(&p.original), &p.choices);
    let current = label(p.current.as_ref(), &p.choices);
    rsx! {
        label{r#for:"slack-list-grouping-field","Group by"}
        select{id:"slack-list-grouping-field",value:"{field}",disabled:!editable,"data-grouping-choice":"{choice}",
            onchange:move |e|{let id=e.value();let desired=if id.is_empty(){ViewGroupingChoice::None}else{ViewGroupingChoice::Column{column_id:id}};grouping::input(state,serial,visit,choice,rendered.clone(),desired);},
            option{value:"",selected:field.is_empty(),"None"}
            for (id,name) in p.choices {option{value:"{id}",selected:field==id,"{name}"}}
        }
        p{id:"slack-list-grouping-current",class:"slack-list-editor-hint","Original: {original}. Current: {current}."}
        p{class:"slack-list-editor-hint",if local {"Apply changes your current view. Use Save for everyone afterward to share it."} else {"Save changes this shared view for everyone with access. Stored values are kept. Choose None to remove grouping."}}
    }
}
#[cfg(test)]
mod tests;
