//! 80bA's directory acquisition branch for the actual ordinary Share preset.
//! rfHj supplies neither allowEmptyQueryRemoteSearch nor entities.lists.
//! This decision does not suppress ranking/cache reads or grant eligibility.
use super::Query;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquisitionRoute {
    Local,
    Remote,
}

impl Query {
    /// `online` is the acquired connectivity comparison with ONLINE.
    /// `client_host` is the explicit sSvv client/client-v2 host observation or
    /// declared native consumer policy; it is not inferred from the host OS.
    /// Neither input has a default. Wire emptiness precedes fuzzy normalization:
    /// a space remains remote online, while a stripped prefix can be local.
    pub fn acquisition_route(&self, online: bool, client_host: bool) -> AcquisitionRoute {
        if !online || (client_host && self.wire().is_empty()) {
            AcquisitionRoute::Local
        } else {
            AcquisitionRoute::Remote
        }
    }
}

#[cfg(test)]
mod tests;
