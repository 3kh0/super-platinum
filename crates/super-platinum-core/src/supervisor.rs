use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct Generation(Arc<AtomicU64>);

impl Default for Generation {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }
}

impl Generation {
    pub fn current(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn advance(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.current() == generation
    }
}

pub struct SupervisorContext {
    shutdown: watch::Receiver<bool>,
    generation: Generation,
    started_generation: u64,
}

impl SupervisorContext {
    pub fn generation(&self) -> u64 {
        self.started_generation
    }

    pub fn is_current(&self) -> bool {
        self.generation.is_current(self.started_generation)
    }

    pub async fn cancelled(&mut self) {
        while !*self.shutdown.borrow() && self.shutdown.changed().await.is_ok() {}
    }
}

pub struct RealtimeSupervisor {
    shutdown: watch::Sender<bool>,
    generation: Generation,
    tasks: Vec<JoinHandle<()>>,
}

impl Default for RealtimeSupervisor {
    fn default() -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            shutdown,
            generation: Generation::default(),
            tasks: Vec::new(),
        }
    }
}

impl RealtimeSupervisor {
    pub fn generation(&self) -> Generation {
        self.generation.clone()
    }

    pub fn restart_generation(&self) -> u64 {
        self.generation.advance()
    }

    pub fn spawn<Worker, Work>(&mut self, worker: Worker)
    where
        Worker: FnOnce(SupervisorContext) -> Work + Send + 'static,
        Work: Future<Output = ()> + Send + 'static,
    {
        let started_generation = self.generation.current();
        let context = SupervisorContext {
            shutdown: self.shutdown.subscribe(),
            generation: self.generation.clone(),
            started_generation,
        };
        self.tasks.push(tokio::spawn(worker(context)));
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }
}
