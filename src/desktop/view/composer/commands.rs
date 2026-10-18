//! Actual discovered command selection, separate from dispatch and native proof.
use super::*;
use crate::drafts::protected::production::commands as controller;

fn chooser(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    let Some(p) = controller::presentation(&state.read(), target) else {
        return rsx! {};
    };
    let owner = p.owner;
    let busy = p.loading || p.running;
    let selected = p.selected.map(|v| v.to_string()).unwrap_or_default();
    let id = if target == ComposerTarget::Thread {
        "thread-command-choice"
    } else {
        "channel-command-choice"
    };
    rsx! {
        section {class:"composer-command-chooser", "aria-label":"Installed app command",
            if p.loading { p {role:"status", "Loading commands…"} }
            if p.running { p {role:"status", "Running command…"} }
            if let Some(message)=p.message { p {role:"status", "{message}"} }
            if !p.choices.is_empty() {
                label {r#for:id, "App command"}
                select {id, value:selected, disabled:busy,
                    onchange:move |event|controller::select(state,target,owner,&event.value()),
                    option {value:"", disabled:true, "Choose an app"}
                    for (index,label) in p.choices {
                        option {key:"{owner}:{index}", value:"{index}", "{label}"}
                    }
                }
                button {class:"composer-command-run", disabled:busy||p.selected.is_none(),
                    onclick:move |_|controller::run(state,target,owner), "Run command"}
            }
            button {disabled:p.running, onclick:move |_|controller::cancel(state,target,owner),"Close"}
        }
    }
}

pub(super) fn panel(state: Signal<ShellState>, target: ComposerTarget) -> Element {
    let retained = controller::recovery::presentation(&state.read(), target);
    rsx! {
        {chooser(state,target)}
        if !retained.is_empty() {
            section {class:"composer-command-chooser", "aria-label":"Saved command completion",
                p {"A command completed. Finish reconciling its saved result without sending it again."}
                for id in retained {
                    button {key:"{id}", onclick:move |_|controller::recovery::finish(state,target,id), "Finish saved command completion"}
                }
            }
        }
    }
}
