//! PdIx/0mwJ source-absent member completion, with explicit NetworkOnly scope.
//! This leaf owns neither the scheduler nor native adoption. A terminal result
//! is a caller observation, never inferred from one failed/invalid response ID.
use super::*;
use crate::state::directory_members::{Observations, Store};

#[cfg(test)]
mod tests;

/// Observed runtime public path used by the retained 9WOq asset factory.
/// There is deliberately no guessed production URL or native-avatar fallback.
pub struct PlaceholderAsset(String);
impl PlaceholderAsset {
    pub fn observed(public_path: &str) -> Result<Self, Error> {
        let parsed = url::Url::parse(public_path).map_err(|_| unavailable())?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !public_path.ends_with('/')
        {
            return Err(unavailable());
        }
        Ok(Self(format!("{public_path}unknown_avatar-0d4e910.png")))
    }
}

/// The parent observes completion of the entire source fetch, including pages,
/// pending rounds and any manager timeout handling. A partial page is not this.
pub enum NetworkTerminal {
    /// Rows after actual xqiG interaction projection, in returned source order.
    Complete(Vec<Value>),
    /// Entire manager fetch rejected, or its timeout retries were exhausted.
    Failed,
}

pub struct Capture {
    before: Observations,
    ids: Vec<String>,
}
impl Capture {
    /// Captures the actual raw-source absence, not absence from Workspace.users.
    /// The caller explicitly chooses native NetworkOnly because no source raw
    /// offline object store exists. CacheOnly/Optimistic are not emulated here.
    pub fn network_only(before: Observations, requested: &[String]) -> Result<Self, Error> {
        if requested.is_empty() || requested.len() > 100_000 {
            return Err(unavailable());
        }
        let existing: BTreeSet<_> = before.source_ids().collect();
        let mut seen = BTreeSet::new();
        let mut ids = Vec::new();
        for id in requested {
            if !member(id) {
                return Err(unavailable());
            }
            if !existing.contains(id.as_str()) && seen.insert(id.as_str()) {
                ids.push(id.clone());
            }
        }
        Ok(Self { before, ids })
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn observations(&self) -> &Observations {
        &self.before
    }
    pub fn reads(&self) -> Result<Vec<Read>, Error> {
        self.pending_reads(&self.ids)
    }
    /// Actual pending retry IDs, bounded to this original source-absent capture.
    /// Empty ends the schedule; changed authority or unrelated IDs cannot be
    /// reconstructed by creating a new capture. Parent still checks currentness.
    pub fn pending_reads(&self, ids: &[String]) -> Result<Vec<Read>, Error> {
        let captured: BTreeSet<_> = self.ids.iter().map(String::as_str).collect();
        let mut seen = BTreeSet::new();
        if ids
            .iter()
            .any(|id| !captured.contains(id.as_str()) || !seen.insert(id.as_str()))
        {
            return Err(unavailable());
        }
        ids.chunks(100)
            .map(|ids| Read::absent(ids.to_vec()))
            .collect()
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> bool {
        self.before
            .current(store, account, owner, epoch, generation)
    }
    /// Returns exact DirectUpsert inputs: fetched rows, then terminal missing
    /// placeholders. They still require ordinary XbX7 renderer hydration.
    /// A changed Store/scope refuses the entire receipt; no late row overwrites.
    pub fn complete(
        self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        terminal: NetworkTerminal,
        asset: &PlaceholderAsset,
    ) -> Result<Vec<Value>, Error> {
        if !self.current(store, account, owner, epoch, generation) {
            return Err(unavailable());
        }
        let mut rows = match terminal {
            NetworkTerminal::Complete(rows) => rows,
            NetworkTerminal::Failed => Vec::new(),
        };
        let requested: BTreeSet<_> = self.ids.iter().map(String::as_str).collect();
        let mut returned = BTreeSet::new();
        for row in &rows {
            let id = row["id"].as_str().ok_or_else(unavailable)?;
            if !row.is_object()
                || !requested.contains(id)
                || !row.get("is_stranger").is_some_and(Value::is_boolean)
            {
                return Err(unavailable());
            }
            returned.insert(id.to_owned());
        }
        for id in self.ids.iter().filter(|id| !returned.contains(*id)) {
            rows.push(placeholder(id, asset, false));
        }
        Ok(rows)
    }
    /// Ephemeral manager projection only. Never enqueue these as known members.
    pub fn unknown_rows(&self, asset: &PlaceholderAsset) -> Vec<Value> {
        self.ids
            .iter()
            .map(|id| placeholder(id, asset, true))
            .collect()
    }
}

fn placeholder(id: &str, asset: &PlaceholderAsset, unknown: bool) -> Value {
    let mut row = json!({
        "id":id,"name":if unknown {id} else {""},"files":[],
        "is_bot":false,"updated":0,"is_app_user":false,
        "profile":{
            "real_name":"","display_name":"","avatar_hash":"",
            "real_name_normalized":"","display_name_normalized":"",
            "image_24":asset.0,"image_32":asset.0,"image_48":asset.0,
            "image_72":asset.0,"image_192":asset.0,"image_512":asset.0,
            "image_1024":asset.0
        }
    });
    row[if unknown {
        "isUnknown"
    } else {
        "isNonExistent"
    }] = json!(true);
    row
}
