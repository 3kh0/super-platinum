//! Feature-only identity carrier with no HTTP capability.
use super::*;

impl Transport {
    /// Synthetic fixtures may capture normal session/Transport identity while
    /// substituting their explicitly named final executor. No client is built.
    pub fn disconnected_fixture(d_cookie: impl Into<String>) -> Self {
        Self {
            http: None,
            canvas_http: Arc::new(std::sync::OnceLock::new()),
            d_cookie: d_cookie.into(),
            health: Arc::new(watch::Sender::new(Health::Unknown)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str) -> PreparedRequest {
        PreparedRequest {
            method: "POST",
            url: format!("https://fixture.slack.com{path}"),
            headers: vec![],
            body: RequestBody::Form(vec![("token".into(), "synthetic".into())]),
        }
    }
    fn refused<T>(result: Result<T, Error>) {
        assert!(
            matches!(result, Err(Error::Offline(ref value)) if value == "disconnected fixture transport")
        );
    }

    #[test]
    fn clones_and_health_changes_never_produce_an_http_client() {
        let value = Transport::disconnected_fixture("synthetic");
        let cloned = value.clone();
        for health in [Health::Unknown, Health::Offline, Health::Online] {
            value.set_health(health);
            assert_eq!(cloned.health(), health);
            assert_eq!(cloned.d_cookie(), "synthetic");
            refused(cloned.http());
            refused(cloned.single_attempt_client());
            assert!(cloned.canvas_http.get().is_none());
        }
        let ordinary = Transport::new("synthetic").unwrap();
        assert!(ordinary.http().is_ok());
        // Even an already populated strict-client cache cannot bypass refusal.
        assert!(
            value
                .canvas_http
                .set(ordinary.http().unwrap().clone())
                .is_ok()
        );
        refused(value.single_attempt_client());
    }

    #[tokio::test]
    async fn generic_history_refuses_before_offline_wait_or_retry() {
        let transport = Transport::disconnected_fixture("synthetic");
        for health in [Health::Unknown, Health::Offline, Health::Online] {
            transport.set_health(health);
            let result = tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute(request("/api/conversations.history")),
            )
            .await
            .unwrap();
            refused(result);
        }
    }

    #[tokio::test]
    async fn media_text_upload_and_canvas_have_no_client_escape() {
        let transport = Transport::disconnected_fixture("synthetic");
        refused(
            transport
                .get_text("https://fixture.invalid/text", "fixture")
                .await,
        );
        refused(
            transport
                .get_bytes("https://fixture.invalid/media", "fixture")
                .await,
        );
        refused(
            transport
                .get_public_bytes("https://fixture.invalid/media", "fixture")
                .await,
        );
        refused(
            transport
                .get_public_gif_bytes("https://fixture.invalid/media", "fixture")
                .await,
        );
        let progress = Arc::new(AtomicU64::new(0));
        refused(
            transport
                .upload_bytes(
                    "https://fixture.invalid/upload",
                    vec![1, 2, 3],
                    progress.clone(),
                )
                .await,
        );
        assert_eq!(progress.load(Ordering::Relaxed), 0);
        refused(
            transport
                .execute_canvas_collab_read(request("/api/quip.getControllerInitData"))
                .await,
        );
    }

    #[tokio::test]
    async fn protected_executor_refusal_remains_not_requested() {
        let transport = Transport::disconnected_fixture("synthetic");
        let error = transport
            .execute_protected_draft_mutation(request("/api/chat.postMessage"), || true)
            .await
            .unwrap_err();
        assert!(!error.requested());
        refused::<()>(Err(error.into_error()));
        assert!(transport.canvas_http.get().is_none());
    }

    #[tokio::test]
    async fn original_download_refuses_before_writing_including_loopback_capability() {
        use crate::file_download::{DownloadError, OriginalSource};
        let path =
            std::env::temp_dir().join(format!("disconnected-original-{}", uuid::Uuid::new_v4()));
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .await
            .unwrap();
        let transport = Transport::disconnected_fixture("synthetic");
        let mut sources = vec![OriginalSource::parse("https://files.slack.com/original").unwrap()];
        #[cfg(feature = "original-download-fixture")]
        sources
            .push(OriginalSource::fixture_loopback(1, "0123456789abcdef0123456789abcdef").unwrap());
        for source in sources.drain(..) {
            let result = transport
                .download_original(&source, "fixture", &mut file, || true)
                .await;
            assert!(
                matches!(result,Err(DownloadError::Network(Error::Offline(ref text))) if text == "disconnected fixture transport")
            );
        }
        assert_eq!(file.metadata().await.unwrap().len(), 0);
        drop(file);
        tokio::fs::remove_file(path).await.unwrap();
    }
}
