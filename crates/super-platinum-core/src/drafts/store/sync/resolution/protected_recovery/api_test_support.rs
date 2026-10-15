//! Explicit isolated test store. Never opens a production path or keyring.
//! Construction and Reply::wait are blocking and belong off renderer threads.
pub use super::faults::FaultPoint;
use super::*;
pub struct Isolated {
    client: Client,
}
impl Isolated {
    pub fn create(account: String, absolute_directory: std::path::PathBuf) -> Result<Self, Error> {
        if !valid_uuid(&account) {
            return Err(Error::Stale);
        }
        let route =
            crate::drafts::store::worker::protected_queue::isolated(&account, absolute_directory)?;
        let client = Client {
            account: account.clone(),
            route,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        client.route.submit(commands::Command {
            account,
            action: commands::Action::Initialize { reply: sender },
        });
        Reply(receiver).wait()?;
        Ok(Self { client })
    }
    pub fn fail_next(&self, point: FaultPoint) -> Reply<()> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.client.route.submit(commands::Command {
            account: self.client.account.clone(),
            action: commands::Action::ArmFault {
                point,
                reply: sender,
            },
        });
        Reply(receiver)
    }
    pub fn client(&self) -> Client {
        self.client.clone()
    }
}
