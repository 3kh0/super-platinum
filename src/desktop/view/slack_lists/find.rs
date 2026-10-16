use super::*;

pub(super) fn local_page(saved: (u64, usize), revision: u64, pages: usize) -> usize {
    if saved.0 == revision {
        saved.1.min(pages.saturating_sub(1))
    } else {
        0
    }
}
#[cfg(test)]
mod tests {
    use super::local_page;
    #[test]
    fn find_resets_both_board_lane_and_card_pages_even_when_matches_span_pages() {
        assert_eq!(local_page((1, 3), 1, 8), 3);
        assert_eq!(local_page((1, 3), 2, 8), 0);
        assert_eq!(local_page((2, 2), 3, 8), 0);
        assert_eq!(local_page((3, 7), 3, 2), 1);
        assert_eq!(local_page((3, 7), 3, 0), 0);
    }
}

pub(super) fn bar(state: Signal<ShellState>, view: &crate::slack_lists::Presentation) -> Element {
    let visit = view.generation;
    rsx! {
        div{class:"slack-list-find",onmounted:move |_|{dioxus::document::eval(include_str!("find.js"));},
            label{r#for:"slack-list-find-input","Find"}
            input{id:"slack-list-find-input",class:"list-find",r#type:"search","aria-label":"Find in loaded items",placeholder:"Find in loaded items",value:"{view.find_input}",
                oninput:move |event|crate::slack_lists::find::input(state,event.value(),visit),
                onkeydown:move |event|{if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();crate::slack_lists::find::input(state,String::new(),visit);}}
            }
            button{id:"slack-list-find-clear",r#type:"button",class:"icon-btn","aria-label":"Clear Find",disabled:view.find_input.is_empty(),
                onclick:move |_|{crate::slack_lists::find::input(state,String::new(),visit);dioxus::document::eval("document.getElementById('slack-list-find-input')?.focus({preventScroll:true});");},
                {icon(Icon::Close,"icon")}
            }
        }
    }
}
