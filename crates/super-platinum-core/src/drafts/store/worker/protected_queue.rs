//! Existing Worker routing. The isolated route is explicit test support only;
//! enabling its feature cannot change the Default route or normal persistence.
#[cfg(any(test, feature = "protected-test-support"))]
use super::super::protected::Error;
use super::super::protected::commands::Command;
use super::*;
#[derive(Clone)]
pub(crate) enum Route {
    Default,
    #[cfg(any(test, feature = "protected-test-support"))]
    Owned(std::sync::Arc<super::owned_queue::Queue>),
    #[cfg(any(test, feature = "protected-test-support"))]
    Isolated(std::sync::Arc<isolated::Queue>),
}
impl Route {
    pub(crate) fn submit(&self, command: Command) {
        match self {
            #[cfg(any(test, feature = "protected-test-support"))]
            Self::Owned(queue) => queue.submit_protected(command),
            Self::Default => worker().submit(Request::Protected(Box::new(command))),
            #[cfg(any(test, feature = "protected-test-support"))]
            Self::Isolated(queue) => queue.worker.submit(Request::Protected(Box::new(command))),
        }
    }
}
#[cfg(any(test, feature = "protected-test-support"))]
mod isolated {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    pub(crate) struct Queue {
        pub(super) worker: Arc<Worker>,
        stopping: Arc<AtomicBool>,
        thread: Mutex<Option<std::thread::JoinHandle<()>>>,
        // Held for the complete route lifetime. Evidence is retained on drop.
        _lock: File,
    }
    impl Drop for Queue {
        fn drop(&mut self) {
            self.stopping.store(true, Ordering::Release);
            self.worker.ready.notify_all();
            if let Some(thread) = self.thread.lock().unwrap().take() {
                let _ = thread.join();
            }
        }
    }
    fn valid_owned(directory: &Path, marker: &str) -> Result<(), AppError> {
        let metadata = std::fs::symlink_metadata(directory)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || std::fs::read_to_string(directory.join("fixture.json"))? != marker
        {
            return Err(sync::invalid("isolated fixture identity changed"));
        }
        for name in ["fixture.json", "drafts.sqlite"] {
            if let Ok(metadata) = std::fs::symlink_metadata(directory.join(name)) {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(sync::invalid("isolated fixture path replaced"));
                }
            }
        }
        Ok(())
    }
    pub(super) fn create(account: &str, directory: PathBuf) -> Result<Route, Error> {
        if uuid::Uuid::parse_str(account).is_err() || !directory.is_absolute() {
            return Err(Error::Stale);
        }
        // Fresh directory is the ownership CAS. Existing empty directories are
        // rejected too; no caller path can silently select a prior database.
        std::fs::create_dir(&directory).map_err(|_| Error::Storage)?;
        let directory = directory.canonicalize().map_err(|_| Error::Storage)?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let lock = options
            .open(directory.join("fixture.lock"))
            .map_err(|_| Error::Storage)?;
        let marker=serde_json::to_string(&serde_json::json!({"version":1,"account":account,"nonce":uuid::Uuid::new_v4().to_string()})).map_err(|_|Error::Storage)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join("fixture.json"))
            .map_err(|_| Error::Storage)?;
        file.write_all(marker.as_bytes())
            .map_err(|_| Error::Storage)?;
        file.sync_all().map_err(|_| Error::Storage)?;
        drop(file);
        let worker = Arc::new(Worker {
            protected_fixture: true,
            ..Worker::default()
        });
        let stopping = Arc::new(AtomicBool::new(false));
        let run = worker.clone();
        let stop = stopping.clone();
        let expected_account = account.to_owned();
        let thread = std::thread::Builder::new()
            .name("protected-isolated-drafts".into())
            .spawn(move || {
                loop {
                    {
                        let mut pending = run.pending.lock().unwrap();
                        while pending.requests.is_empty()
                            && pending.updates.is_empty()
                            && !stop.load(Ordering::Acquire)
                        {
                            pending = run.ready.wait(pending).unwrap();
                        }
                        if stop.load(Ordering::Acquire) && pending.requests.is_empty() {
                            break;
                        }
                    }
                    let retry = run.cycle(|account| {
                        if account != expected_account {
                            return Err(sync::invalid("isolated account changed"));
                        }
                        valid_owned(&directory, &marker)?;
                        DraftStore::open(directory.join("drafts.sqlite"))
                    });
                    if retry {
                        // Same Worker retry policy, with a bounded wake for teardown.
                        let pending = run.pending.lock().unwrap();
                        let _ = run
                            .ready
                            .wait_timeout(pending, std::time::Duration::from_secs(1))
                            .unwrap();
                    }
                }
            })
            .map_err(|_| Error::Storage)?;
        Ok(Route::Isolated(Arc::new(Queue {
            worker,
            stopping,
            thread: Mutex::new(Some(thread)),
            _lock: lock,
        })))
    }
}
#[cfg(any(test, feature = "protected-test-support"))]
pub(crate) fn isolated(account: &str, directory: std::path::PathBuf) -> Result<Route, Error> {
    isolated::create(account, directory)
}
