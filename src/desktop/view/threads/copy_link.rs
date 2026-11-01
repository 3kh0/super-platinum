//! Quiet inline Copy controls; the owned native bridge supplies the gesture.
use super::*;
use crate::thread_feed::copy_link as copy;
pub(super) fn render(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    action: Option<copy::Action>,
) -> Element {
    let Some(action) = action else {
        return if copy::layout(snapshot) {
            rsx! {span{class:"thread-copy-control",
                button{r#type:"button",class:"thread-feed-open",disabled:true,
                    {crate::icons::icon(crate::icons::Icon::Link,"icon sm")}"Copy link"}
                span{class:"secondary-meta",role:"status","Link ready. Choose Copy link to update the clipboard."}
            }}
        } else {
            rsx! {}
        };
    };
    let p = copy::presentation(snapshot, &action);
    if let Some(p) = p.as_ref().filter(|p| p.ready) {
        return rsx! {ReadyControl{key:"{p.id}",state,id:p.id,scope:p.scope.clone(),notice:p.notice.clone(),copying:p.copying,native_ready:p.native_ready}};
    }
    let preparing = p.as_ref().is_some_and(|p| p.preparing);
    let notice = p.map(|p| p.notice);
    rsx! {span{class:"thread-copy-control",
        button{r#type:"button",class:"thread-feed-open",id:action.button_id(),disabled:preparing,"aria-busy":preparing,
            onclick:move|e|{e.stop_propagation();copy::prepare(state,action.clone());},
            {crate::icons::icon(crate::icons::Icon::Link,"icon sm")}"Copy link"
        }
        if let Some(notice)=notice{span{class:"secondary-meta",role:"status","{notice}"}}
    }}
}
#[component]
fn ReadyControl(
    state: Signal<ShellState>,
    id: u64,
    scope: String,
    notice: String,
    copying: bool,
    native_ready: bool,
) -> Element {
    let mount = use_hook(copy::next_mount);
    let config = copy::Config::new(id, mount, scope.clone());
    let rendered = rsx! {span{id:config.host_id.clone(),class:"thread-copy-control","data-thread-copy-scope":scope,"data-thread-copy-intent":"{id}","data-thread-copy-mount":"{mount}",
        onmounted:move |_|copy::mounted(state,id,mount),
        // Keep the physical button enabled during a permitted promise. The JS
        // control drops queued peers and rechecks disabled before its API call.
        button{id:config.button_id,r#type:"button",class:"thread-feed-open",disabled:!native_ready,"aria-busy":copying,
            {crate::icons::icon(crate::icons::Icon::Link,"icon sm")}"Copy link"
        }
        span{class:"secondary-meta",role:"status","{notice}"}
    }};
    #[cfg(test)]
    copy::test_io::record(&rendered);
    rendered
}
