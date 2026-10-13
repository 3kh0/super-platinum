//! Snapshot-wide eligibility projection without rescanning titles per item.
use super::*;

impl CheckboxIntent {
    /// Capture renderer-selected exact IDs and desired values in input order.
    /// Builds one section index and shares permission/title work across all
    /// targets. No Automerge state or edit counters are created by projection.
    pub fn capture_batch(
        snapshot: &CanvasCollabSnapshot,
        targets: &[(&str, bool)],
        now_usec: u64,
    ) -> Vec<Result<Self, Error>> {
        if targets.is_empty() {
            return Vec::new();
        }
        let common = (|| {
            eligible_document(snapshot)?;
            let title = Arc::new(CanonicalTitle::capture(snapshot)?);
            let user = snapshot.init.options().string(33)?.ok_or(Error::Changed)?;
            Ok::<_, Error>((title, user))
        })();
        let (title, user) = match common {
            Ok(common) => common,
            Err(error) => return targets.iter().map(|_| Err(error)).collect(),
        };
        let mut sections = HashMap::with_capacity(snapshot.sections.len());
        for section in &snapshot.sections {
            // Canonical snapshots have unique IDs. Retaining the first entry
            // also matches Snapshot::section if a synthetic caller duplicates one.
            sections.entry(section.id()).or_insert(section);
        }
        let mut parents = HashMap::<&str, Arc<Section>>::new();
        targets
            .iter()
            .map(|(id, desired)| {
                let (section, parent) =
                    eligible_section(snapshot, id, now_usec, |id| sections.get(id).copied())?;
                if section.checked()?.unwrap_or(false) == *desired {
                    return Err(Error::Changed);
                }
                let parent = parents
                    .entry(parent.id())
                    .or_insert_with(|| Arc::new(parent.clone()));
                Ok(Self {
                    file: snapshot.file_id().to_owned(),
                    thread: snapshot.thread_id().to_owned(),
                    document: snapshot.document_id().to_owned(),
                    section: section.clone(),
                    parent: Arc::clone(parent),
                    title: Arc::clone(&title),
                    workspace: snapshot.init.workspace_id().to_owned(),
                    user: user.to_owned(),
                    desired: *desired,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
