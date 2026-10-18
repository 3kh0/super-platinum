use super::*;
use crate::slack_lists::assignment::{self, Presentation};

#[component]
pub(super) fn AssignmentReview(state: Signal<ShellState>, review: Presentation) -> Element {
    let serial = review.serial;
    rsx! {
        section {class:"slack-list-assignment-review",id:"slack-list-assignment-review",
            "aria-labelledby":"slack-list-assignment-title","aria-busy":review.busy,
            "data-assignment-serial":serial,
            h4 {id:"slack-list-assignment-title","Share this List with the people you assigned?"}
            p {class:"slack-list-editor-hint","Assigning: {review.selected}"}
            p {class:"slack-list-editor-hint",role:"status","{review.message}"}
            for person in review.people {
                {
                    let id=person.id.clone();
                    let label=format!("Access for {}",person.label);
                    rsx! {
                        label {class:"slack-list-assignment-person",key:"{person.id}",
                            span {"{person.label}"}
                            select {"data-assignment-user":person.id.clone(),"aria-label":label,value:person.grant.as_str(),disabled:!review.can_share || person.status!="Not shared",
                                onchange:move |event|assignment::grant(state,serial,id.clone(),event.value()),
                                option {value:"read",selected:person.grant==super_platinum_core::slack::api::AssigneeGrant::Read,"Can view"}
                                option {value:"write",selected:person.grant==super_platinum_core::slack::api::AssigneeGrant::Write,"Can edit"}
                            }
                            span {class:"slack-list-editor-hint","{person.status}"}
                        }
                    }
                }
            }
            p {class:"slack-list-editor-hint","Abandon stops this assignment and retains any earlier sharing result."}
            {recovery_action(state)}
            div {class:"slack-list-editor-actions",
                button {"data-assignment-action":String::from("check"),id:"slack-list-assignment-check",r#type:"button",disabled:!review.can_check,
                    onclick:move |_|assignment::check(state,serial),"Check access"}
                button {"data-assignment-action":String::from("not-now"),id:"slack-list-assignment-not-now",r#type:"button",disabled:!review.can_not_now,
                    onclick:move |_|assignment::not_now(state,serial),"Not Now"}
                button {"data-assignment-action":String::from("share"),id:"slack-list-assignment-share",r#type:"button",class:"primary",disabled:!review.can_share,
                    onclick:move |_|assignment::share(state,serial),"Share"}
            }
        }
    }
}

#[cfg(test)]
mod tests;

/// Also rendered in the List notices when the originating cell no longer exists.
/// Receipt retirement does not require the obsolete cell to be reopened.
pub(in crate::view::slack_lists) fn recovery_notice(state: Signal<ShellState>) -> Element {
    let Some(recovery) = assignment::recovery(&state.read()).filter(|r| !r.in_editor) else {
        return rsx! {};
    };
    let serial = recovery.serial;
    rsx! {
        div {class:"slack-list-warning",id:"slack-list-assignment-recovery","data-assignment-serial":serial,
            onmounted:move |_| { dioxus::document::eval(RECOVERY_FOCUS); },
            p {"An assignment review is retained. You can abandon the assignment while keeping any earlier sharing result."}
            {recovery_action(state)}
        }
    }
}
fn recovery_action(state: Signal<ShellState>) -> Element {
    let Some(recovery) = assignment::recovery(&state.read()) else {
        return rsx! {};
    };
    rsx! {
        div {class:"slack-list-editor-actions",
            button {id:"slack-list-assignment-abandon","data-assignment-action":String::from("abandon"),
                r#type:"button",disabled:!recovery.can_abandon,
                onclick:move |_|assignment::abandon(state,recovery.clone()),"Abandon assignment"}
        }
    }
}
// The editor already owns focus return for its action. A detached review notice
// returns only its own actual focus to Refresh (or Close) in the same List visit.
const RECOVERY_FOCUS: &str = r#"(()=>{
 const root=document.getElementById('slack-list-assignment-recovery');
 const dialog=root?.closest('.slack-list-dialog');
 if(!root||!dialog)return;
 const visit=dialog.dataset.dialogGeneration;
 let owned=false;
 const focus=e=>{if(e.target!==document.body)owned=root.contains(e.target);};
 const observer=new MutationObserver(()=>{
   if(root.isConnected&&dialog.isConnected)return;
   observer.disconnect();document.removeEventListener('focusin',focus,true);
   if(!owned||!dialog.isConnected||dialog.dataset.dialogGeneration!==visit||document.getElementById('shell-overlay-dialog')!==dialog||document.activeElement!==document.body)return;
   const target=[dialog.querySelector('#slack-list-refresh'),dialog.querySelector('#slack-list-close')].find(e=>e&&!e.disabled&&e.getClientRects().length&&!e.closest('[hidden],[inert]'));
   target?.focus({preventScroll:true});
 });
 document.addEventListener('focusin',focus,true);
 observer.observe(document.body,{childList:true,subtree:true});
})();"#;
