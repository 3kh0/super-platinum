//! Captured source facts and exact renderer lowercase acquisition.
use super::projection::{self, Context, Error, Processed};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub self_id: String,
    pub current_team: String,
    pub omit_team: String,
    pub retain_public_membership_on_archive: bool,
    /// Canonical source member observations, not default-filled typed users.
    pub members: BTreeMap<String, Value>,
}
impl Facts {
    fn context<'a>(
        &'a self,
        boot_in_flight: bool,
        lowercase: &'a dyn Fn(&str) -> Option<String>,
    ) -> Context<'a> {
        Context {
            self_id: &self.self_id,
            current_team: &self.current_team,
            omit_team: &self.omit_team,
            boot_in_flight,
            retain_public_membership_on_archive: self.retain_public_membership_on_archive,
            members: &self.members,
            lowercase,
        }
    }
}
pub struct Plan {
    facts: Facts,
    boot_in_flight: bool,
    generation: String,
    before: Vec<Value>,
    incoming: Vec<Value>,
    fields: Vec<String>,
}
/// The host returns the exact fields and generation along with ordinary JS
/// `String.prototype.toLowerCase` results. Query-locale normalization differs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererReply {
    pub generation: String,
    pub fields: Vec<String>,
    pub values: Vec<String>,
}
impl Plan {
    pub fn capture(
        facts: Facts,
        boot_in_flight: bool,
        generation: &str,
        before: &[Value],
        incoming: &[Value],
    ) -> Result<Self, Error> {
        if generation.is_empty() || generation.len() > 1024 {
            return Err(Error::Shape);
        }
        let fields = RefCell::new(Vec::new());
        let seen = RefCell::new(BTreeSet::new());
        let observe = |raw: &str| {
            let mut fields = fields.borrow_mut();
            if raw.len() > 65_536 || fields.len() >= 100_000 {
                return None;
            }
            if seen.borrow_mut().insert(raw.to_owned()) {
                fields.push(raw.to_owned());
            }
            // Only the lowercase output slot is substituted. It does not affect
            // subsequent row/name computation: 4XO+ uses one before snapshot.
            Some(raw.to_owned())
        };
        let processed =
            projection::batch(before, incoming, &facts.context(boot_in_flight, &observe))?;
        // EmXR indexes raw processed names. Ordinary unknown channels may have
        // _name_lc="" from 4XO+ while still having a nonempty indexable name.
        // Capture that distinct ordinary lowercase observation explicitly.
        for row in processed.updates() {
            if let Some(name) = super::names::raw_name(row)? {
                observe(name).ok_or(Error::MissingObservation)?;
            }
        }
        Ok(Self {
            facts,
            boot_in_flight,
            generation: generation.into(),
            before: before.to_vec(),
            incoming: incoming.to_vec(),
            fields: fields.into_inner(),
        })
    }
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[cfg(test)]
    pub fn apply(self, current: &Facts, reply: RendererReply) -> Result<Processed, Error> {
        self.apply_with_names(current, reply)
            .map(|(processed, _)| processed)
    }
    pub fn apply_with_names(
        self,
        current: &Facts,
        reply: RendererReply,
    ) -> Result<(Processed, BTreeMap<String, String>), Error> {
        if current != &self.facts
            || reply.generation != self.generation
            || reply.fields != self.fields
            || reply.values.len() != self.fields.len()
            || reply.values.iter().any(|s| s.len() > 65_536)
        {
            return Err(Error::MissingObservation);
        }
        let values: BTreeMap<_, _> = self.fields.into_iter().zip(reply.values).collect();
        let lowercase = |raw: &str| values.get(raw).cloned();
        let processed = projection::batch(
            &self.before,
            &self.incoming,
            &self.facts.context(self.boot_in_flight, &lowercase),
        )?;
        Ok((processed, values))
    }
}
