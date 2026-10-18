//! Repeated explicit single-file Insert, unique append, exact reorder/removal.
//! This is editor content only; no persistence or network authority.
use super::*;
use crate::slack::{
    api::file_share::batch::MAX_EXISTING_FILES,
    file_share_policy::{Eligibility, source},
    models::File,
};
use serde_json::Value;
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Selection {
    sources: Vec<Value>,
}
impl Selection {
    pub fn sources(&self) -> &[Value] {
        &self.sources
    }
    pub fn ids(&self) -> Vec<String> {
        self.sources
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_owned())
            .collect()
    }
    pub fn from_input(input: &Input) -> Self {
        Self {
            sources: input.sources().to_vec(),
        }
    }
    pub fn insert(&self, raw: Value) -> Result<Self, Error> {
        let id = valid(&raw)?;
        if let Some(old) = self.sources.iter().find(|v| v["id"].as_str() == Some(id)) {
            return if old == &raw {
                Ok(self.clone())
            } else {
                Err(Error::Stale)
            };
        }
        if self.sources.len() >= MAX_EXISTING_FILES {
            return Err(Error::Unsupported);
        }
        let mut next = self.clone();
        next.sources.push(raw);
        next.validate()?;
        Ok(next)
    }
    pub fn remove(&self, id: &str) -> Result<Self, Error> {
        if !self.sources.iter().any(|v| v["id"].as_str() == Some(id)) {
            return Err(Error::Stale);
        }
        Ok(Self {
            sources: self
                .sources
                .iter()
                .filter(|v| v["id"].as_str() != Some(id))
                .cloned()
                .collect(),
        })
    }
    pub fn reorder(&self, ids: &[String]) -> Result<Self, Error> {
        if ids.len() != self.sources.len()
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        {
            return Err(Error::Stale);
        }
        let sources = ids
            .iter()
            .map(|id| {
                self.sources
                    .iter()
                    .find(|v| v["id"].as_str() == Some(id))
                    .cloned()
                    .ok_or(Error::Stale)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { sources })
    }
    pub fn input(&self, review: &Review) -> Result<Input, Error> {
        self.validate()?;
        let mut snapshot = review.row().state().desired().cloned().unwrap_or_else(|| {
            crate::drafts::sync::Snapshot {
                blocks: vec![],
                file_ids: vec![],
                destinations: vec![serde_json::json!({"channel_id":review.scope().key.channel})],
            }
        });
        if !snapshot.file_ids.is_empty() {
            return Err(Error::Unsupported);
        }
        snapshot.file_ids = self.ids();
        let input = Input::new(snapshot, self.sources.clone())?;
        input.paired(review)?;
        Ok(input)
    }
    fn validate(&self) -> Result<(), Error> {
        if self.sources.len() > MAX_EXISTING_FILES
            || !model::bounded(&Value::Array(self.sources.clone()), 2 * 1024 * 1024)
        {
            return Err(Error::Unsupported);
        }
        let mut ids = std::collections::BTreeSet::new();
        for raw in &self.sources {
            if !ids.insert(valid(raw)?) {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }
}
fn valid(raw: &Value) -> Result<&str, Error> {
    if !raw.is_object() || !model::bounded(raw, 256 * 1024) {
        return Err(Error::Unsupported);
    }
    let id = raw
        .get("id")
        .and_then(Value::as_str)
        .ok_or(Error::Unsupported)?;
    if !(2..=64).contains(&id.len())
        || !id.starts_with('F')
        || !id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(Error::Unsupported);
    }
    let file: File = serde_json::from_value(raw.clone()).map_err(|_| Error::Unsupported)?;
    if source(&file) != Eligibility::Allowed {
        return Err(Error::Unsupported);
    }
    Ok(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn file(id: &str) -> Value {
        json!({"id":id,"mode":"hosted","is_external":false,"filetype":"pdf","title":id})
    }
    #[test]
    fn insert_is_single_append_unique_and_reorder_is_exact() {
        let s = Selection::default()
            .insert(file("FB"))
            .unwrap()
            .insert(file("FA"))
            .unwrap();
        assert_eq!(s.ids(), vec!["FB", "FA"]);
        assert_eq!(s.insert(file("FB")).unwrap().ids(), vec!["FB", "FA"]);
        let mut changed = file("FB");
        changed["title"] = json!("changed");
        assert!(s.insert(changed).is_err());
        assert_eq!(
            s.reorder(&["FA".into(), "FB".into()]).unwrap().ids(),
            vec!["FA", "FB"]
        );
        assert!(s.reorder(&["FA".into(), "FA".into()]).is_err());
        assert!(s.reorder(&["FA".into(), "FC".into()]).is_err());
        assert_eq!(s.remove("FB").unwrap().ids(), vec!["FA"]);
        assert!(s.remove("FC").is_err());
    }
    #[test]
    fn unsupported_external_or_deleted_sources_never_enter_the_set() {
        for field in ["is_external", "is_tombstoned"] {
            let mut raw = file("FA");
            raw[field] = json!(true);
            assert!(Selection::default().insert(raw).is_err());
        }
        assert!(
            Selection::default()
                .insert(json!({"id":"FA","mode":"snippet"}))
                .is_err()
        );
    }
}
