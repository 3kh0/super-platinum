//! Channel and member suggestions. Native owns the caret ticket; rows contain
//! labels and an index into the host's private revisioned completion catalog.
use super::*;
use crate::profile_edit::sections::rich::{self as controller, query, search};
use serde::Deserialize;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
struct Popup {
    sequence: u64,
    revision: u64,
    labels: Vec<String>,
    selected: usize,
    status: String,
    member: bool,
}
#[derive(Default)]
struct Runtime {
    live: Cell<bool>,
    sequence: Cell<u64>,
    eval: RefCell<Option<dioxus::document::Eval>>,
    lease: RefCell<Option<query::Lease>>,
    task: RefCell<Option<dioxus::core::Task>>,
}
impl Runtime {
    fn retire(&self, mut state: Signal<ShellState>) {
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
        if let Some(lease) = self.lease.borrow_mut().take() {
            query::close(&mut state.write(), &lease);
        }
    }
    fn stop(&self, state: Signal<ShellState>) {
        self.live.set(false);
        self.retire(state);
        if let Some(eval) = self.eval.borrow_mut().take() {
            let _ = eval.send(json!({"kind":"stop"}));
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Stopped,
    Query {
        sequence: u64,
        trigger: String,
        query: String,
        before: String,
        token: String,
        index: usize,
        revision: u64,
    },
    Closed {
        sequence: u64,
    },
    Highlight {
        sequence: u64,
        revision: u64,
        index: usize,
    },
    Choose {
        sequence: u64,
        revision: u64,
        index: usize,
    },
    Inserted {
        sequence: u64,
        accepted: bool,
    },
}

#[component]
pub(super) fn Suggestions(
    state: Signal<ShellState>,
    visit: u64,
    serial: u64,
    mount: u64,
) -> Element {
    let mut shown = use_signal(Popup::default);
    let runtime = use_hook(|| {
        Rc::new(Runtime {
            live: Cell::new(true),
            ..Runtime::default()
        })
    });
    let cleanup = runtime.clone();
    use_drop(move || cleanup.stop(state));
    let watched = runtime.clone();
    use_effect(move || {
        let current = controller::mount_current(&state.read(), visit, serial, mount);
        if !current && watched.live.get() {
            watched.stop(state);
            shown.set(Popup::default());
        }
    });
    let running = runtime.clone();
    use_future(move || {
        let runtime = running.clone();
        async move {
            let script = format!(
                r#"
                const matcher=({matcher})({data});
                for(let i=0;i<60&&!document.getElementById('profile-section-rich');i++)await new Promise(r=>setTimeout(r,16));
                const control=({bridge})({visit},{serial},{mount},{{}},matcher,event=>dioxus.send(event));
                if(control){{try{{while(control.current())control.message(await dioxus.recv());}}finally{{control.stop();}}}}
            "#,
                matcher = include_str!("rich/trigger.js"),
                data = include_str!("rich/trigger-data.json"),
                bridge = include_str!("rich/completion.js")
            );
            let mut eval = dioxus::document::eval(&script);
            *runtime.eval.borrow_mut() = Some(eval);
            while let Ok(event) = eval.recv::<Event>().await {
                if !runtime.live.get()
                    || !controller::mount_current(&state.peek(), visit, serial, mount)
                {
                    break;
                }
                match event {
                    Event::Stopped => break,
                    Event::Query {
                        sequence,
                        trigger,
                        query: term,
                        before,
                        token,
                        index,
                        revision,
                    } => {
                        if sequence <= runtime.sequence.get()
                            || sequence >= 9_007_199_254_740_991
                            || revision >= 9_007_199_254_740_991
                            || !matches!(trigger.as_str(), "channel" | "member")
                            || before.len() > 1_048_576
                            || token.len() > 1_048_576
                            || !trigger_matches(&trigger, &token)
                            || term != token.chars().skip(1).collect::<String>()
                            || index.checked_add(token.encode_utf16().count())
                                != Some(before.encode_utf16().count())
                            || !before.ends_with(&token)
                        {
                            continue;
                        }
                        let member = trigger == "member";
                        runtime.sequence.set(sequence);
                        runtime.retire(state);
                        let mut state = state;
                        let lease =
                            query::begin(&mut state.write(), visit, serial, mount, sequence);
                        *runtime.lease.borrow_mut() = lease.clone();
                        shown.set(Popup {
                            sequence,
                            status: if member {
                                "Searching people…"
                            } else {
                                "Searching channels…"
                            }
                            .into(),
                            member,
                            ..Popup::default()
                        });
                        let Some(lease) = lease else {
                            shown.set(Popup::default());
                            continue;
                        };
                        let task_runtime = runtime.clone();
                        let results_runtime = runtime.clone();
                        let results_lease = lease.clone();
                        let task = spawn(async move {
                            let on_results = move |choices| {
                                if results_runtime.live.get()
                                    && results_runtime.sequence.get() == sequence
                                    && results_lease.current(&state.peek())
                                {
                                    publish(eval, shown, choices, member);
                                }
                            };
                            let result = if member {
                                search::run_members(state, lease.clone(), term, on_results).await
                            } else {
                                search::run(state, lease.clone(), term, on_results).await
                            };
                            if task_runtime.live.get()
                                && task_runtime.sequence.get() == sequence
                                && lease.current(&state.peek())
                            {
                                if let Err(error) = result {
                                    let mut popup = shown.write();
                                    popup.status=if matches!(error,super_platinum_core::slack::Error::RateLimited{..}) {
                                        "Slack asked us to wait before searching again."
                                    } else if member { "People suggestions could not load. Edit your query to try again." } else { "Channel suggestions could not load. Edit your query to try again." }.into();
                                }
                            }
                        });
                        *runtime.task.borrow_mut() = Some(task);
                    }
                    Event::Closed { sequence } => {
                        if sequence <= runtime.sequence.get() || sequence >= 9_007_199_254_740_991 {
                            continue;
                        }
                        runtime.sequence.set(sequence);
                        runtime.retire(state);
                        shown.set(Popup::default());
                    }
                    Event::Highlight {
                        sequence,
                        revision,
                        index,
                    } => {
                        let mut popup = shown.write();
                        if popup.sequence == sequence
                            && popup.revision == revision
                            && index < popup.labels.len()
                        {
                            popup.selected = index;
                        }
                    }
                    Event::Choose {
                        sequence,
                        revision,
                        index,
                    } => {
                        if sequence != runtime.sequence.get() {
                            continue;
                        }
                        let lease = runtime.lease.borrow().clone();
                        let Some(lease) = lease else {
                            continue;
                        };
                        let mut state = state;
                        let completion = query::choose(&mut state.write(), &lease, revision, index);
                        runtime.retire(state);
                        if let Some(completion) = completion {
                            let value =
                                serde_json::to_value(completion).expect("serializable completion");
                            let _=eval.send(json!({"kind":"insert","sequence":sequence,"revision":revision,"index":index,
                                "value":value["value"],"label":value["label"],"suffix":" "}));
                        } else {
                            let _=eval.send(json!({"kind":"rejected","sequence":sequence,"revision":revision,"index":index}));
                        }
                        shown.set(Popup {
                            sequence,
                            ..Popup::default()
                        });
                    }
                    Event::Inserted { sequence, accepted } => {
                        if sequence == runtime.sequence.get() && !accepted {
                            shown.set(Popup {sequence,status:"The suggestion could not be inserted. Review the description before continuing.".into(),..Popup::default()});
                        }
                    }
                }
            }
            runtime.stop(state);
            shown.set(Popup::default());
        }
    });
    let view = shown();
    rsx! {
        div { class:"profile-rich-completions",
            "data-query-sequence":view.sequence.to_string(),
            "data-query-revision":view.revision.to_string(),
            if !view.labels.is_empty() {
                div { id:"profile-rich-choices-{mount}",class:"profile-rich-options",role:"listbox","aria-label":if view.member {"People suggestions"} else {"Channel suggestions"},
                    for (index,label) in view.labels.iter().enumerate() {
                        button { key:"{view.sequence}-{view.revision}-{index}",id:"profile-rich-choice-{mount}-{view.sequence}-{index}",r#type:"button",role:"option",
                            class:if index==view.selected {"profile-rich-option selected"} else {"profile-rich-option"},
                            "aria-selected":(index==view.selected).to_string(),tabindex:"-1",
                            onmousedown:move |e|e.prevent_default(),
                            onclick:{let runtime=runtime.clone();let sequence=view.sequence;let revision=view.revision;move |_| {
                                if let Some(eval)=runtime.eval.borrow().as_ref() { let _=eval.send(json!({"kind":"choose","sequence":sequence,"revision":revision,"index":index})); }
                            }},
                            {icon(if view.member {Icon::AlternateEmail} else {Icon::Tag},"icon profile-rich-channel-icon")}
                            span { class:"profile-rich-option-label","{label}" }
                        }
                    }
                }
            }
            if !view.status.is_empty() { p { class:"profile-rich-status",role:"status","{view.status}" } }
        }
    }
}
fn publish(
    eval: dioxus::document::Eval,
    mut shown: Signal<Popup>,
    choices: query::Presentation,
    member: bool,
) {
    let count = choices.labels.len();
    if eval.send(json!({"kind":"choices","sequence":choices.sequence,"revision":choices.revision,"count":count})).is_err() { return; }
    shown.set(Popup {
        sequence: choices.sequence,
        revision: choices.revision,
        labels: choices.labels,
        selected: 0,
        member,
        status: if count == 0 {
            if member {
                "No matching people"
            } else {
                "No matching channels"
            }
            .into()
        } else {
            String::new()
        },
    });
}

fn trigger_matches(trigger: &str, token: &str) -> bool {
    matches!(
        (trigger, token.chars().next()),
        ("channel", Some('#' | '\u{ff03}')) | ("member", Some('@' | '\u{ff20}'))
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn query_category_and_token_prefix_must_agree() {
        for (category, token) in [
            ("channel", "#project"),
            ("channel", "\u{ff03}project"),
            ("member", "@name"),
            ("member", "\u{ff20}name"),
        ] {
            assert!(super::trigger_matches(category, token));
        }
        for (category, token) in [
            ("channel", "@name"),
            ("member", "#project"),
            ("file", "@name"),
            ("member", "name"),
            ("member", ""),
        ] {
            assert!(!super::trigger_matches(category, token));
        }
    }
}
