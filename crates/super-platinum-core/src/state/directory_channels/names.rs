//! EmXR channelNamesToIds: lazy runtime aliases, updated by actual channel
//! actions. This is neither a second channel collection nor a persisted index.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Im,
    Channel,
    Mpim,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Index {
    maps: BTreeMap<Kind, BTreeMap<String, String>>,
    reverse: BTreeMap<String, Vec<(Kind, String)>>,
    // Exact ordinary-renderer lowercase for each current canonical row name.
    // eH changes only membership, so it reuses the same observed name pair.
    lower_by_id: BTreeMap<String, (String, String)>,
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
pub(super) fn raw_name(row: &Value) -> Result<Option<&str>> {
    match row.get("name") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok((!s.is_empty()).then_some(s.as_str())),
        _ => Err(Error::Shape),
    }
}
fn optional_text<'a>(row: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok((!s.is_empty()).then_some(s.as_str())),
        _ => Err(Error::Shape),
    }
}
impl Index {
    pub(super) fn update(
        &mut self,
        rows: &[Value],
        lower: &BTreeMap<String, String>,
    ) -> Result<()> {
        for row in rows {
            let id = projection::id(row)?;
            let Some(name) = raw_name(row)? else { continue };
            if truth(&row["fromAnotherTeam"]) {
                continue;
            }
            let lowered = lower
                .get(name)
                .cloned()
                .or_else(|| {
                    self.lower_by_id
                        .get(id)
                        .filter(|(old, _)| old == name)
                        .map(|(_, v)| v.clone())
                })
                .ok_or(Error::MissingObservation)?;
            self.lower_by_id
                .insert(id.into(), (name.into(), lowered.clone()));
            let im_peer = if truth(&row["is_im"]) {
                optional_text(row, "user")?
            } else {
                None
            };
            // The lexical winner guard happens before removing this row's old
            // aliases. A losing same-peer update therefore retains its alias.
            if let Some(peer) = im_peer {
                if self
                    .maps
                    .get(&Kind::Im)
                    .and_then(|m| m.get(peer))
                    .is_some_and(|old| old.as_str() > id)
                {
                    continue;
                }
            }
            self.remove_aliases(id);
            let (kind, mut keys) = if let Some(peer) = im_peer {
                (Kind::Im, vec![peer.to_owned()])
            } else if truth(&row["is_mpim"]) {
                match optional_text(row, "_internal_name")? {
                    Some(internal) => (Kind::Mpim, vec![internal.to_owned()]),
                    None => (Kind::Channel, vec![]),
                }
            } else {
                (Kind::Channel, vec![])
            };
            keys.push(lowered);
            for key in &keys {
                self.maps
                    .entry(kind)
                    .or_default()
                    .insert(key.clone(), id.into());
            }
            self.reverse
                .insert(id.into(), keys.into_iter().map(|key| (kind, key)).collect());
        }
        Ok(())
    }
    fn remove_aliases(&mut self, id: &str) {
        if let Some(keys) = self.reverse.get_mut(id) {
            for (kind, key) in keys.drain(..) {
                // Source deletes the key unconditionally, even if another row
                // most recently wrote it. Do not repair this alias collision.
                if let Some(map) = self.maps.get_mut(&kind) {
                    map.remove(&key);
                }
            }
        }
    }
    pub(super) fn remove(&mut self, id: &str) {
        self.remove_aliases(id);
        self.lower_by_id.remove(id);
    }
    fn member(&self, id: &str) -> Result<Option<&str>> {
        // Member selector inputs are canonical ASCII identities; this exactly
        // implements ordinary JS lowercase for this closed input domain.
        if id.len() < 2
            || id.len() > 255
            || !matches!(id.as_bytes()[0], b'U' | b'W')
            || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(Error::Shape);
        }
        let lowered = id.to_ascii_lowercase();
        for kind in [Kind::Im, Kind::Channel, Kind::Mpim] {
            if let Some(map) = self.maps.get(&kind) {
                if let Some(value) = map.get(&lowered).or_else(|| map.get(id)) {
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }
}

/// First lookup captures names from the actual complete Ready order. The caller
/// supplies the existing renderer's ordinary lowercase under this exact lease.
pub struct NamePlan {
    stamp: Stamp,
    native: BTreeMap<String, Value>,
    rows: Vec<Value>,
    fields: Vec<String>,
}
impl NamePlan {
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    pub fn generation(&self) -> &str {
        &self.stamp.generation
    }
}

pub struct NameSnapshot {
    stamp: Stamp,
    index: Arc<Index>,
    rows: BTreeMap<String, Value>,
}
impl NameSnapshot {
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> bool {
        store.names.is_some()
            && store.current(&self.stamp, account, owner, epoch, generation)
            && store.native_current(native)
    }
    /// Actual EmXR.Pu probes IM, channel and MPIM aliases, in that order. It
    /// returns the original canonical row, not a synthesized D from a U ID.
    pub fn for_member(&self, member: &str) -> Result<Option<&Value>> {
        self.index
            .member(member)?
            .map(|id| self.rows.get(id).ok_or(Error::MissingObservation))
            .transpose()
    }
}

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_names(
        &self,
        ready: &Ready,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Option<NamePlan>> {
        if !ready.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        if self.names.is_some() {
            return Ok(None);
        }
        let mut fields = Vec::new();
        let mut seen = BTreeSet::new();
        for row in ready.rows() {
            if let Some(name) = raw_name(row)? {
                if name.len() > 65_536 {
                    return Err(Error::Shape);
                }
                if seen.insert(name.to_owned()) {
                    fields.push(name.to_owned());
                }
            }
        }
        Ok(Some(NamePlan {
            stamp: self.stamp(account, owner, epoch, generation)?,
            native: native_snapshot(native)?,
            rows: ready.rows().to_vec(),
            fields,
        }))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finish_names(
        &mut self,
        plan: NamePlan,
        reply: RendererReply,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<NameSnapshot> {
        if !self.current(&plan.stamp, account, owner, epoch, generation)
            || native_snapshot(native)? != plan.native
            || !self.native_current(native)
            || reply.generation != plan.stamp.generation
            || reply.fields != plan.fields
            || reply.values.len() != plan.fields.len()
            || reply.values.iter().any(|v| v.len() > 65_536)
        {
            return Err(Error::MissingObservation);
        }
        // Another same-source consumer may have initialized the same lazy map
        // while this renderer reply was pending. Never replace its live map.
        if self.names.is_none() {
            let lower = plan.fields.into_iter().zip(reply.values).collect();
            let mut index = Index::default();
            index.update(&plan.rows, &lower)?;
            self.names = Some(Arc::new(index));
        }
        self.names_snapshot(account, owner, epoch, generation, native)
    }
    pub fn names_snapshot(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<NameSnapshot> {
        let ready = self.ready(account, owner, epoch, generation, native)?;
        Ok(NameSnapshot {
            stamp: ready.stamp,
            index: self.names.clone().ok_or(Error::MissingObservation)?,
            rows: ready
                .rows
                .into_iter()
                .map(|r| (projection::id(&r).unwrap().into(), r))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
