use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{Command, CommandAction, UiCommand, UiCommandResult};

pub type Reducer<State, Message> =
    dyn Fn(&mut State, Message) -> Command<Message> + Send + Sync + 'static;

pub trait UiCommandExecutor: Send + Sync + 'static {
    fn execute(
        &self,
        command: UiCommand,
    ) -> Pin<Box<dyn Future<Output = UiCommandResult> + Send + 'static>>;
}

pub struct DispatchHandle<Message> {
    sender: mpsc::UnboundedSender<Message>,
}

impl<Message> Clone for DispatchHandle<Message> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Message> DispatchHandle<Message> {
    pub fn dispatch(&self, message: Message) -> Result<(), mpsc::error::SendError<Message>> {
        self.sender.send(message)
    }
}

pub struct AppDispatcher<State, Message> {
    state: Arc<Mutex<State>>,
    handle: DispatchHandle<Message>,
    task: JoinHandle<()>,
}

impl<State, Message> AppDispatcher<State, Message>
where
    State: Send + 'static,
    Message: Send + 'static,
{
    pub fn spawn(
        state: State,
        reducer: Arc<Reducer<State, Message>>,
        ui: Arc<dyn UiCommandExecutor>,
    ) -> Self {
        let state = Arc::new(Mutex::new(state));
        let (sender, mut receiver) = mpsc::unbounded_channel::<Message>();
        let loop_sender = sender.clone();
        let loop_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                let command = {
                    let mut state = loop_state.lock().expect("dispatcher state poisoned");
                    reducer(&mut state, message)
                };
                for action in command.into_actions() {
                    match action {
                        CommandAction::Done(message) => {
                            let _ = loop_sender.send(message);
                        }
                        CommandAction::Perform(future) => {
                            let sender = loop_sender.clone();
                            tokio::spawn(async move {
                                let _ = sender.send(future.await);
                            });
                        }
                        CommandAction::Ui(request) => {
                            let sender = loop_sender.clone();
                            let ui = Arc::clone(&ui);
                            tokio::spawn(async move {
                                let result = ui.execute(request.command).await;
                                if let Some(resolve) = request.resolve {
                                    let _ = sender.send(resolve(result));
                                }
                            });
                        }
                    }
                }
            }
        });
        Self {
            state,
            handle: DispatchHandle { sender },
            task,
        }
    }

    pub fn handle(&self) -> DispatchHandle<Message> {
        self.handle.clone()
    }

    pub fn with_state<Output>(&self, inspect: impl FnOnce(&State) -> Output) -> Output {
        inspect(&self.state.lock().expect("dispatcher state poisoned"))
    }

    pub fn abort(self) {
        self.task.abort();
    }
}
