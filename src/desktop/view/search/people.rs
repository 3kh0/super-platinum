use super::*;
mod numbered;
pub(crate) use numbered::controls as numbered_controls;
pub(crate) use numbered::sort_controls;

pub(crate) fn results(state: Signal<ShellState>, shell: &ShellState) -> Element {
    if crate::bootstrap::search_people::numbered::active(shell) {
        return numbered::results(state, shell);
    }
    rsx! {
        for user in crate::bootstrap::search_people::rows(shell) {
            if let Some(source)=crate::bootstrap::search_people::Source::capture(shell,&user.id) {
                button {key:"{user.id}",class:"search-result","data-search-person-id":"{user.id}",
                    onclick:move |_|crate::bootstrap::search_people::open(state,source.clone()),
                    strong {{super_platinum_core::state::display_name(Some(user.as_ref()),&user.id)}}
                    if let Some(title)=user.profile.as_ref().and_then(|p|p.title.as_ref()).filter(|s|!s.trim().is_empty()) {
                        span {"{title}"}
                    }
                    if let Some(email)=user.profile.as_ref().and_then(|p|p.email.as_ref()).filter(|s|!s.trim().is_empty()) {
                        span {class:"search-time","{email}"}
                    }
                }
            }
        }
    }
}
