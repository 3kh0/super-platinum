//! Files-only staged category controls and original-intent submission dispatch.
use super::*;
use crate::state::search_filter_types as intent;
use super_platinum_core::search_query::file_types::FileCategory;
pub(super) fn panel(mut state: Signal<ShellState>, s: &ShellState) -> Element {
    if s.search_pager.kind != SearchKind::Files {
        return rsx! {};
    }
    let source = intent::Source::capture(s);
    let clear = source.clone();
    rsx! {div{class:"search-filter-options",role:"group","aria-label":"File types",
        for kind in FileCategory::ALL{
            label{input{r#type:"checkbox",id:"search-file-type-{kind.value()}","aria-label":kind.label(),checked:intent::selected(s,kind),disabled:source.is_none(),
                onchange:{let source=source.clone();move|event|{if let Some(source)=source.as_ref(){intent::set(&mut state.write(),source,kind,event.checked());}}}
            }"{kind.label()}"}
        }
        button{r#type:"button",class:"chip",id:"search-file-types-clear",disabled:clear.is_none(),onclick:move |_|{if let Some(source)=clear.as_ref(){intent::clear(&mut state.write(),source,false);}},"Clear selected types"}
    }}
}
pub(super) fn submit(
    mut state: Signal<ShellState>,
    source: Option<intent::Source>,
    allow_generic: bool,
) {
    if let Some(source) = source {
        let submission = intent::apply(&mut state.write(), &source);
        if let Some(submission) = submission {
            #[cfg(test)]
            let _io = dioxus::prelude::try_consume_context::<
                std::rc::Rc<crate::bootstrap::search_file_type_io::Io>,
            >()
            .expect("File category UI test must provide scoped offline read IO");
            spawn(async move {
                if submission.current(&state.peek()) {
                    crate::bootstrap::search(state).await;
                }
            });
        }
    } else {
        if !allow_generic {
            return;
        }
        let allowed = {
            let s = state.peek();
            intent::allows(&s, None)
        };
        if allowed && state.write().apply_search_filters() {
            spawn(crate::bootstrap::search(state));
        }
    }
}
pub(super) fn clear(
    mut state: Signal<ShellState>,
    source: Option<intent::Source>,
    allow_generic: bool,
) {
    if let Some(source) = source {
        intent::clear(&mut state.write(), &source, true);
    } else {
        if !allow_generic {
            return;
        }
        if !intent::allows(&state.peek(), None) {
            return;
        }
        state.write().search_filters = Some(SearchFilters::default());
        if state.write().apply_search_filters() {
            spawn(crate::bootstrap::search(state));
        }
    }
}
