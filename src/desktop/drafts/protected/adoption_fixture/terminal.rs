//! Buffer the terminal fixture receipt off the UI thread. Its exact JSON line
//! remains the runner's only terminal evidence; no synthetic fallback receipt.
use serde_json::Value;
use std::io::{self, Write};
const PREFIX: &[u8] = b"protected-adoption-check: ";

pub(super) async fn emit(value: Value) -> Result<(), String> {
    emit_with(value, |bytes| io::stderr().lock().write_all(bytes)).await
}

async fn emit_with<F>(value: Value, write: F) -> Result<(), String>
where
    F: FnOnce(&[u8]) -> io::Result<()> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut bytes = Vec::from(PREFIX);
        serde_json::to_writer(&mut bytes, &value).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        // One buffered write_all, with the stderr lock acquired only by emit's
        // sink here. Encoding, writing and dropping the owned Value all happen
        // on this worker, including failed serialization/write paths.
        write(&bytes).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Terminal logging task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn success_and_error_receipts_match_original_bytes_once_off_caller() {
        for value in [
            json!({"ok":true,"snapshots":[{"text":"\u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nline\""}],"padding":"x".repeat(600_000)}),
            json!({"ok":false,"error":"Adoption checker timed out"}),
            json!({"ok":false,"error":"A real native failure"}),
        ] {
            let expected = format!("protected-adoption-check: {value}\n").into_bytes();
            let caller = std::thread::current().id();
            let writes = Arc::new(Mutex::new(Vec::new()));
            let observed = writes.clone();
            emit_with(value, move |bytes| {
                assert_ne!(std::thread::current().id(), caller);
                observed.lock().unwrap().push(bytes.to_vec());
                Ok(())
            })
            .await
            .unwrap();
            assert_eq!(*writes.lock().unwrap(), vec![expected]);
        }
    }

    #[tokio::test]
    async fn failed_writer_propagates_without_replay_or_success_fallback() {
        let attempts = Arc::new(Mutex::new(0));
        let observed = attempts.clone();
        let error = emit_with(json!({"ok":true}), move |_| {
            *observed.lock().unwrap() += 1;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "closed fixture log",
            ))
        })
        .await
        .unwrap_err();
        assert!(error.contains("closed fixture log"));
        assert_eq!(*attempts.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn awaited_worker_completion_is_finite_and_observable() {
        let (sent, received) = tokio::sync::oneshot::channel();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            emit_with(json!({"ok":false,"error":"native refusal"}), move |bytes| {
                sent.send(bytes.ends_with(b"\n")).unwrap();
                Ok(())
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(received.await.unwrap());
    }
}
