//! Shared inert canvas/text-layer controls. No document action toolbar.
use dioxus::prelude::*;
pub(super) fn view(dom_id: String, mounted: EventHandler<MountedEvent>) -> Element {
    rsx! {
        div {id:"{dom_id}",class:"pdf-preview",onmounted:mounted,
            div {class:"pdf-toolbar","aria-label":"PDF controls",
                button {r#type:"button","data-pdf":"previous",disabled:true,"Previous"}
                label {"Page",input {r#type:"number",min:"1",value:"1",disabled:true,"aria-label":"PDF page","data-pdf":"page"}}
                span {"data-pdf":"count","of …"}
                button {r#type:"button","data-pdf":"next",disabled:true,"Next"}
                label {"Zoom",select {"data-pdf":"zoom",disabled:true,"aria-label":"PDF zoom",
                    option {value:"1","Fit width"}option {value:"1.5","150%"}option {value:"2","200%"}option {value:"3","300%"}
                }}
                button {r#type:"button","data-pdf":"rotate",disabled:true,"Rotate"}
            }
            div {class:"pdf-search",role:"search","aria-label":"Find in PDF",
                input {r#type:"text",role:"searchbox",placeholder:"Find in PDF",disabled:true,"data-pdf":"find","aria-label":"Find in PDF",autocomplete:"off",spellcheck:false}
                span {role:"status","data-pdf":"find-status"}
                button {r#type:"button",class:"pdf-find-previous",disabled:true,"data-pdf":"find-previous","aria-label":"Previous PDF match",title:"Previous match (Shift+Enter)",{crate::icons::icon(crate::icons::Icon::ChevronRight,"icon")}}
                button {r#type:"button",disabled:true,"data-pdf":"find-next","aria-label":"Next PDF match",title:"Next match (Enter)",{crate::icons::icon(crate::icons::Icon::ChevronRight,"icon")}}
                button {r#type:"button",disabled:true,"data-pdf":"find-clear","aria-label":"Clear PDF search",title:"Clear search",{crate::icons::icon(crate::icons::Icon::Close,"icon")}}
            }
            p {class:"pdf-status",role:"status","data-pdf":"status","Loading PDF…"}
            form {class:"pdf-password",hidden:true,"data-pdf":"password-form",
                label {"Password",input {r#type:"password",autocomplete:"off","data-pdf":"password","aria-label":"PDF password"}}
                button {r#type:"submit","Unlock"}
            }
            div {class:"pdf-scroll",tabindex:"0","aria-label":"PDF document",
                div {class:"pdf-page",canvas {"aria-label":"PDF page image"}div {class:"textLayer"}}
            }
        }
    }
}
