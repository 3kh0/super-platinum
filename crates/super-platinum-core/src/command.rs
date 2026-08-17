use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Renderer-neutral asynchronous work returned by a reducer.
pub struct Command<Message> {
    actions: Vec<CommandAction<Message>>,
}

pub enum CommandAction<Message> {
    Done(Message),
    Perform(BoxFuture<Message>),
    Ui(UiCommandRequest<Message>),
}

pub struct UiCommandRequest<Message> {
    pub command: UiCommand,
    pub resolve: Option<Box<dyn FnOnce(UiCommandResult) -> Message + Send + 'static>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    Focus { target: String },
    ScrollTo { target: String, x: f64, y: f64 },
    ScrollBy { target: String, x: f64, y: f64 },
    ReadClipboardText,
    ReadClipboardFiles,
    WriteClipboardText(String),
    PickFiles { multiple: bool },
    OpenExternal { url: String },
    CaptureScreenshot { path: Option<PathBuf> },
    CloseWindow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiCommandResult {
    Completed,
    Text(Option<String>),
    Files(Vec<PathBuf>),
    Screenshot(CapturedScreenshot),
    Failed(String),
}

/// Renderer-neutral RGBA window capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedScreenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl CapturedScreenshot {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, String> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(format!(
                "invalid RGBA capture length: expected {expected}, got {}",
                rgba.len()
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

impl<Message> Default for Command<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> Command<Message> {
    pub fn none() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    pub fn done(message: Message) -> Self {
        Self {
            actions: vec![CommandAction::Done(message)],
        }
    }

    pub fn perform<Future, Output, Mapper>(future: Future, mapper: Mapper) -> Self
    where
        Future: std::future::Future<Output = Output> + Send + 'static,
        Output: Send + 'static,
        Mapper: FnOnce(Output) -> Message + Send + 'static,
    {
        Self {
            actions: vec![CommandAction::Perform(Box::pin(async move {
                mapper(future.await)
            }))],
        }
    }

    pub fn ui<Resolver>(command: UiCommand, resolver: Resolver) -> Self
    where
        Resolver: FnOnce(UiCommandResult) -> Message + Send + 'static,
    {
        Self {
            actions: vec![CommandAction::Ui(UiCommandRequest {
                command,
                resolve: Some(Box::new(resolver)),
            })],
        }
    }

    pub fn ui_discard(command: UiCommand) -> Self {
        Self {
            actions: vec![CommandAction::Ui(UiCommandRequest {
                command,
                resolve: None,
            })],
        }
    }

    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        Self {
            actions: commands
                .into_iter()
                .flat_map(|command| command.actions)
                .collect(),
        }
    }

    pub fn map<Other, Mapper>(self, mapper: Mapper) -> Command<Other>
    where
        Message: Send + 'static,
        Other: Send + 'static,
        Mapper: Fn(Message) -> Other + Clone + Send + Sync + 'static,
    {
        let actions = self
            .actions
            .into_iter()
            .map(|action| match action {
                CommandAction::Done(message) => CommandAction::Done(mapper(message)),
                CommandAction::Perform(future) => {
                    let mapper = mapper.clone();
                    CommandAction::Perform(Box::pin(async move { mapper(future.await) }))
                }
                CommandAction::Ui(request) => {
                    let mapper = mapper.clone();
                    CommandAction::Ui(UiCommandRequest {
                        command: request.command,
                        resolve: request.resolve.map(|resolve| {
                            Box::new(move |result| mapper(resolve(result)))
                                as Box<dyn FnOnce(UiCommandResult) -> Other + Send>
                        }),
                    })
                }
            })
            .collect();
        Command { actions }
    }

    pub fn is_none(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn into_actions(self) -> impl Iterator<Item = CommandAction<Message>> {
        self.actions.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn batches_and_maps_each_action_kind() {
        let command = Command::batch([
            Command::done(1_u8),
            Command::perform(async { 2_u8 }, |value| value),
            Command::ui(UiCommand::ReadClipboardText, |_| 3_u8),
        ])
        .map(|value| u16::from(value) + 10);

        let mut values = Vec::new();
        for action in command.into_actions() {
            values.push(match action {
                CommandAction::Done(value) => value,
                CommandAction::Perform(future) => future.await,
                CommandAction::Ui(request) => request.resolve.unwrap()(UiCommandResult::Completed),
            });
        }
        assert_eq!(values, [11, 12, 13]);
    }

    #[test]
    fn screenshot_rejects_mismatched_rgba_length() {
        assert!(CapturedScreenshot::new(2, 2, vec![0; 16]).is_ok());
        assert_eq!(
            CapturedScreenshot::new(2, 2, vec![0; 15]).unwrap_err(),
            "invalid RGBA capture length: expected 16, got 15"
        );
    }
}
