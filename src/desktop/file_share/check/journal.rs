//! Read the actual private journal; no result or phase injection.
use super::*;
use super_platinum_core::file_sharing::{NotSent, Scope, State as ReceiptState, store::Journal};
use super_platinum_core::slack::api::file_share::FileShareDisposition;
pub(super) fn label(state: &ReceiptState) -> &'static str {
    match state {
        ReceiptState::NotSent(NotSent::AdmissionCanceled) => "not-sent-admission-canceled",
        ReceiptState::Server(FileShareDisposition::Accepted { .. }) => "accepted",
        ReceiptState::Server(FileShareDisposition::DlpWarning { .. }) => "dlp-warning",
        ReceiptState::Server(FileShareDisposition::DlpProcessing) => "processing",
        ReceiptState::Unconfirmed => "unconfirmed",
        _ => "unexpected",
    }
}
pub(super) async fn read(capability: &fixture::Capability) -> Result<Value, String> {
    let path = capability.storage_path().to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<Value,String>{
        let journal=Journal::open(path,fixture::ACCOUNT).map_err(|_|"Private journal open failed")?;
        let scope=Scope{team:fixture::TEAM.into(),user:fixture::USER.into(),file:fixture::FILE.into(),destination:fixture::DESTINATION.into()};
        let receipt=journal.get(&scope).map_err(|_|"Private journal read failed")?.ok_or("Admitted receipt missing")?;
        let unresolved=journal.unresolved(fixture::TEAM,fixture::USER).map_err(|_|"Unresolved journal read failed")?;
        Ok(json!({"attempt":receipt.attempt_id,"state":label(&receipt.state()),"note":receipt.intent.note(),"destination":receipt.scope.destination,"file":receipt.scope.file,"unresolved":unresolved.len()}))
    }).await.map_err(|_|"Journal task failed")?
}
pub(super) fn expected(case: fixture::Case) -> &'static str {
    match case {
        fixture::Case::Accepted => "accepted",
        fixture::Case::DlpWarning => "dlp-warning",
        fixture::Case::Processing => "processing",
        fixture::Case::Unconfirmed | fixture::Case::StorageFailure => "unconfirmed",
        fixture::Case::SourceStale => "not-sent-admission-canceled",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_source_oracle_requires_cancellation_not_merely_zero_dispatch() {
        assert_eq!(
            expected(fixture::Case::SourceStale),
            label(&ReceiptState::NotSent(NotSent::AdmissionCanceled))
        );
        assert_ne!(
            expected(fixture::Case::SourceStale),
            label(&ReceiptState::Unconfirmed)
        );
        assert_ne!(
            expected(fixture::Case::SourceStale),
            label(&ReceiptState::NotSent(NotSent::Offline))
        );
    }
}
