use super::*;

mod conversation;
mod discovery;
mod runtime;
mod workspace;

pub(super) fn update_inner(app: &mut App, message: Message) -> Task<Message> {
    match &message {
        Message::Conversation(_) => conversation::update(app, message),
        Message::Workspace(_) => workspace::update(app, message),
        Message::Discovery(_) => discovery::update(app, message),
        Message::Runtime(_) => runtime::update(app, message),
        Message::AccountScoped(_, _) => Task::none(),
    }
}
