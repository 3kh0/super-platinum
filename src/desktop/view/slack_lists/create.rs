//! Header creation uses one explicit action; opening a List never writes.
use super::*;

pub(super) fn action(state: Signal<ShellState>, visit: u64, layout: u64) -> Element {
    let view = crate::slack_lists::create::presentation(&state.read());
    let title = view.reason.as_deref().unwrap_or("Add a new item");
    rsx! {button{id:"slack-list-add-item",r#type:"button",class:"slack-list-action",disabled:!view.enabled,title,
        "aria-describedby":"slack-list-create-help",onclick:move |_|crate::slack_lists::create::add_item(state,visit,layout),
        {icon(Icon::Add,"icon")}"Add item"
    }}
}
pub(super) fn status(state: Signal<ShellState>, visit: u64) -> Element {
    let view = crate::slack_lists::create::presentation(&state.read());
    rsx! {
        if let Some(operation)=view.operation {
            div{id:"slack-list-create-status",class:"slack-list-create-status",role:"status","aria-busy":operation.pending,
                span{"{operation.message}"}
                if operation.can_cancel {button{id:"slack-list-create-cancel",r#type:"button",class:"slack-list-action",onclick:move |_|crate::slack_lists::create::cancel_create(state,operation.serial),"Cancel"}}
                if operation.can_retire {button{id:"slack-list-create-retire",r#type:"button",class:"slack-list-action",onclick:move |_|crate::slack_lists::create::retire_uncertain(state,operation.serial),"I checked Slack"}}
                if operation.can_dismiss {button{id:"slack-list-create-dismiss",r#type:"button",class:"slack-list-action",onclick:move |_|crate::slack_lists::create::dismiss_confirmed(state,operation.serial),"Dismiss"}}
            }
        } else if let Some(notice)=view.notice {
            div{id:"slack-list-create-status",class:"slack-list-create-status",role:"status",span{"{notice}"}
                if view.can_open {button{id:"slack-list-create-open",r#type:"button",class:"slack-list-action",onclick:move |_|crate::slack_lists::create::open_result(state,visit),"Open item"}}
            }
        }
        p{id:"slack-list-create-help",class:if view.reason.is_some(){"slack-list-create-help"}else{"slack-list-create-help slack-list-create-help-hidden"},
            if let Some(reason)=view.reason {"{reason}"}else{"Adds a new item, then opens its fields."}
        }
    }
}
