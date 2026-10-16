//! Submission memory limits, separate from transaction-side authority/CAS.
use super::*;
const BODY: usize = 256 * 1024;
const LABEL: usize = 16 * 1024;

pub(super) fn visit(visit: &Visit) -> bool {
    let key = &visit.key.0;
    [&key.account, &key.team, &key.user]
        .into_iter()
        .all(|s| s.len() <= 512)
        && visit.key.1.len() <= 64
        && admission::bounded(&visit.source, BODY, 32)
}
fn content(content: &Content) -> Result<(), Refusal> {
    if !admission::bounded(&content.source, BODY, 32) || content.title.len() > LABEL {
        return Err(Refusal::Source);
    }
    if content
        .destination
        .as_ref()
        .is_some_and(|d| d.id.len() > 64 || d.name.len() > LABEL)
    {
        return Err(Refusal::Destination);
    }
    // Check borrowed serialization and depth before cloning into the typed note
    // validator. No arbitrarily large incoming value is retained or cloned.
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or(std::io::ErrorKind::FileTooLarge)?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    if content.note.len() > BODY
        || !content.note.iter().all(|v| admission::bounded(v, BODY, 32))
        || serde_json::to_writer(Budget(BODY), &content.note).is_err()
    {
        return Err(Refusal::Note);
    }
    if !content.note.is_empty() {
        super::super::api::file_share::FileShareNote::new(content.note.clone())
            .map_err(|_| Refusal::Note)?;
    }
    Ok(())
}
fn draft(draft: &Draft) -> Result<(), Refusal> {
    if draft.scope.team.len() > 512
        || draft.scope.user.len() > 512
        || draft.scope.file.len() > 64
        || draft.token.len() > 36
    {
        return Err(Refusal::Changed);
    }
    content(&draft.content)
}
pub(super) fn command(command: &Command) -> Result<(), Refusal> {
    match command {
        Command::Read => Ok(()),
        Command::Retain { content: value, .. } => content(value),
        Command::LoadClaim { initial } => content(initial),
        Command::Claim { expected, initial } => {
            if let Some(expected) = expected {
                draft(expected)?;
            }
            content(initial)
        }
        Command::Save {
            expected,
            content: value,
        } => {
            draft(expected)?;
            content(value)
        }
        Command::Clear {
            expected,
            content: value,
        } => {
            draft(expected)?;
            content(value)?;
            if !value.empty() {
                return Err(Refusal::Note);
            }
            if value.source != expected.content.source || value.title != expected.content.title {
                return Err(Refusal::Changed);
            }
            Ok(())
        }
        Command::Discard { expected } => draft(expected),
    }
}
