use super::{Fault, Result, Store, model};

impl Store {
    /// Account-scoped application storage. Invoke only on a blocking worker.
    /// Uses the established configuration namespace; never reads credentials.
    /// Unix file mode is enforced by `open`; Windows inherits the user-profile
    /// directory ACL, as other existing local journals do. No encryption claim.
    pub fn open_default(account: &str) -> Result<Self> {
        if cfg!(test) {
            return Err(Fault::Invalid);
        }
        model::uuid(account)?;
        let directory = crate::config::config_dir().map_err(|_| Fault::Invalid)?;
        Self::open(
            directory.join(format!("canvas-creation-{account}.sqlite")),
            account,
        )
    }
}
