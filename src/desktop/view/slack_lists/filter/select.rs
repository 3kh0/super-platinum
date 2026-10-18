//! Native checkbox choices participate in the same complete-row Save snapshot.
use super::*;
use super_platinum_core::slack_lists::SelectFilterOperator as SelectOp;
pub(super) fn label(op: SelectOp) -> &'static str {
    match op {
        SelectOp::Contains => "includes",
        SelectOp::DoesNotContain => "does not include",
        SelectOp::IsEmpty => "is empty",
        SelectOp::IsNotEmpty => "is not empty",
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    structure: u64,
    id: u64,
    epoch: u64,
    operator: SelectOp,
    selected: Vec<String>,
    choices: Vec<(String, String)>,
    editable: bool,
) -> Element {
    let operators = [
        SelectOp::Contains,
        SelectOp::DoesNotContain,
        SelectOp::IsEmpty,
        SelectOp::IsNotEmpty,
    ];
    rsx! {
        label{r#for:"filter-operator-{id}","Condition"}
        select{id:"filter-operator-{id}","data-filter-operator":"true",value:operator.as_str(),disabled:!editable,onchange:move|event|{if let Some(op)=SelectOp::parse(&event.value()){focus::action(state,serial,visit,structure,filter::Action::SelectOperator(id,epoch,op),None);}},
            for op in operators {option{value:op.as_str(),selected:op==operator,"{label(op)}"}}
        }
        if operator.requires_values(){
            div{class:"slack-list-filter-select-options",role:"group","aria-label":"Select options",
                if choices.is_empty(){p{class:"slack-list-editor-hint","No options in this field."}}
                for (index,(choice,text)) in choices.into_iter().enumerate(){
                    label{key:"{choice}",r#for:"filter-choice-{id}-{index}",
                        input{id:"filter-choice-{id}-{index}",r#type:"checkbox","data-filter-choice":choice.clone(),checked:selected.contains(&choice),disabled:!editable,oninput:move|event|{if filter::update_select(&state.peek(),serial,visit,id,epoch,&choice,event.checked()){let mut state=state;state.write();}}}
                        span{"{text}"}
                    }
                }
                p{class:"slack-list-editor-hint","No selected options leaves this condition neutral."}
            }
        }
    }
}
