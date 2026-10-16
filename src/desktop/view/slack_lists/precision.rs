use super::*;
use crate::slack_lists::rename::precision;

pub(super) fn action(
    state: Signal<ShellState>,
    column: &crate::slack_lists::ColumnView,
    visit: u64,
) -> Element {
    let id = column.id.clone();
    let disabled = !precision::can_open(&state.read(), &id);
    rsx! {button{r#type:"button",class:"slack-list-action slack-list-rename-header",disabled,
    title:"Number format for {column.label}","data-precision-column":"{column.id}",
    "aria-label":"Number format for {column.label}",
    onclick:move |_|precision::open(state,id.clone(),visit),
    span{"{column.label}"}{icon(Icon::Compose,"icon")}}}
}
pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    p: precision::Presentation,
    editable: bool,
) -> Element {
    let current = p
        .current
        .map(|v| v.to_string())
        .unwrap_or_else(|| "Unavailable".into());
    rsx! {
        p{class:"slack-list-editor-item","Field: {p.field}"}
        label{r#for:"slack-list-precision","Decimals"}
        select{id:"slack-list-precision",value:"{p.desired}",disabled:!editable,
            onchange:move |event|{if let Some(value)=parse(&event.value()){precision::input(state,serial,visit,value);}},
            for (value,label) in [(0,"None"),(1,".0"),(2,".00"),(3,".000"),(4,".0000"),(5,".00000")] {
                option{value:"{value}",selected:p.desired==value,"{label}"}
            }
        }
        p{class:"slack-list-editor-hint","Original decimals: {p.original}. Current decimals: {current}."}
        p{class:"slack-list-editor-hint","This changes how numbers are displayed. Stored numbers are unchanged."}
    }
}
fn parse(value: &str) -> Option<u8> {
    match value {
        "0" => Some(0),
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        "5" => Some(5),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_select_values_do_not_coerce_invalid_dom_input() {
        for value in 0..=5 {
            assert_eq!(parse(&value.to_string()), Some(value));
        }
        for value in [
            "", " 2", "2 ", "02", "+2", "2.0", "6", "-1", "255", "number",
        ] {
            assert_eq!(parse(value), None);
        }
    }
}
