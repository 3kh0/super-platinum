use super::*;
use super_platinum_core::file_sharing::store::drafts::{Claim, Mutation};
fn scope(job: &Job) -> DraftScope {
    DraftScope {
        team: job.key.0.team.clone(),
        user: job.key.0.user.clone(),
        file: job.key.1.clone(),
    }
}
fn bound(job: &Job, expected: &Draft) -> bool {
    expected.scope == scope(job) && expected.content.source == job.visit.source
}
fn content(job: &Job, value: &Content) -> bool {
    value.source == job.visit.source
}
pub(super) async fn run(job: &Job) -> Outcome {
    let Some(command) = job.command.get().cloned() else {
        return Outcome::Failed(Refusal::Changed);
    };
    let valid = match &command {
        Command::Read => true,
        Command::Retain { content: value, .. } => content(job, value),
        Command::LoadClaim { initial } => {
            job.visit.writable() && content(job, initial) && initial.empty()
        }
        Command::Claim { expected, initial } => {
            job.visit.writable()
                && content(job, initial)
                && initial.empty()
                && expected.as_ref().is_none_or(|d| bound(job, d))
        }
        Command::Save {
            expected,
            content: value,
        } => job.visit.writable() && bound(job, expected) && content(job, value),
        Command::Clear {
            expected,
            content: value,
        } => {
            job.visit.writable()
                && bound(job, expected)
                && content(job, value)
                && value.empty()
                && value.title == expected.content.title
        }
        Command::Discard { expected } => bound(job, expected),
    };
    if !valid {
        return Outcome::Failed(Refusal::Changed);
    }
    if let Command::Retain { reason, .. } = &command {
        return Outcome::Failed(*reason);
    }
    let scope = scope(job);
    let result = job
        .storage
        .run(move |journal| {
            Ok(match command {
                Command::Read => Outcome::Read(journal.draft(&scope)?),
                Command::Retain { reason, .. } => Outcome::Failed(reason),
                Command::LoadClaim { initial } => {
                    if let Some(receipt) = journal
                        .unresolved(&scope.team, &scope.user)?
                        .into_iter()
                        .find(|r| r.scope.file == scope.file)
                    {
                        return Ok(Outcome::Blocked(receipt));
                    }
                    let expected = journal.draft(&scope)?;
                    if let Some(old) = expected.as_ref().filter(|d| {
                        d.content.source != initial.source || d.content.title != initial.title
                    }) {
                        Outcome::Review(old.clone())
                    } else {
                        claim(
                            journal.claim_draft(scope, expected.as_ref(), initial)?,
                            expected.is_none(),
                        )
                    }
                }
                Command::Claim { expected, initial } => claim(
                    journal.claim_draft(scope, expected.as_ref(), initial)?,
                    expected.is_none(),
                ),
                Command::Save { expected, content } => {
                    match journal.save_draft(&expected, content)? {
                        Mutation::Applied(draft) => Outcome::Saved(draft),
                        Mutation::Conflict(current) => Outcome::Conflict(current),
                        Mutation::Blocked(receipt) => Outcome::Blocked(receipt),
                    }
                }
                Command::Clear { expected, .. } | Command::Discard { expected } => {
                    match journal.discard_draft(&expected)? {
                        Mutation::Applied(()) => Outcome::Discarded,
                        Mutation::Conflict(current) => Outcome::Conflict(current),
                        Mutation::Blocked(receipt) => Outcome::Blocked(receipt),
                    }
                }
            })
        })
        .await;
    result.unwrap_or_else(Outcome::Failed)
}
fn claim(result: Claim, created: bool) -> Outcome {
    match result {
        Claim::Claimed(draft) => Outcome::Claimed { draft, created },
        Claim::Conflict(current) => Outcome::Conflict(current),
        Claim::Blocked(receipt) => Outcome::Blocked(receipt),
        Claim::Capacity => Outcome::Capacity,
    }
}
pub(super) async fn cancel(job: &Job, outcome: Outcome) -> Outcome {
    let Outcome::Claimed { draft, .. } = outcome else {
        return outcome;
    };
    let cleanup = if !draft.content.empty() {
        Cleanup::KeptNonempty
    } else {
        let expected = draft.clone();
        match job
            .storage
            .run(move |journal| journal.discard_draft(&expected))
            .await
        {
            Ok(Mutation::Applied(())) => Cleanup::Removed,
            Ok(Mutation::Conflict(current)) => Cleanup::Conflict(current),
            Ok(Mutation::Blocked(receipt)) => Cleanup::Blocked(receipt),
            Err(error) => Cleanup::Failed(error),
        }
    };
    Outcome::Canceled {
        claim: draft,
        cleanup,
    }
}
