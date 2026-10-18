//! One explicit removal, two source-defined services, no replay of either write.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeStage {
    Ready,
    DisableUnconfirmed,
    Disabled,
    RemoveUnconfirmed,
    Complete,
}
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeRemoval {
    base: Inventory,
    id: String,
    file: String,
    stage: KnowledgeStage,
}
impl KnowledgeRemoval {
    pub fn capture(base: Inventory, id: &str) -> Result<Self, Fault> {
        let row = base.canvas_record(id)?;
        let file = file_id(row).ok_or(Fault::Shape)?.to_owned();
        let knowledge = base
            .properties
            .get("channel_knowledge_canvas")
            .filter(|v| v.is_object())
            .ok_or(Fault::Unsupported)?;
        if knowledge["file_id"].as_str() != Some(file.as_str())
            || !matches!(
                knowledge.get("is_enabled"),
                None | Some(Value::Null) | Some(Value::Bool(true))
            )
        {
            return Err(Fault::Unsupported);
        }
        Ok(Self {
            base,
            id: id.into(),
            file,
            stage: KnowledgeStage::Ready,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub fn stage(&self) -> KnowledgeStage {
        self.stage
    }
    pub fn base(&self) -> &Inventory {
        &self.base
    }
    fn disabled(&self) -> Inventory {
        let mut expected = self.base.clone();
        expected.properties["channel_knowledge_canvas"]["is_enabled"] = false.into();
        expected
    }
    fn removed(&self) -> Inventory {
        let mut expected = self.disabled();
        expected.rows.retain(|r| id(r) != Some(self.id.as_str()));
        expected.properties["tabz"] = Value::Array(expected.rows.clone());
        expected
    }
    /// Consume the one authorized disable before handing its request to Transport.
    pub fn begin_disable(
        &mut self,
        current: &Inventory,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Fault> {
        if self.stage != KnowledgeStage::Ready || current != &self.base {
            return Err(Fault::Changed);
        }
        self.stage = KnowledgeStage::DisableUnconfirmed;
        Ok(client.rest_form(
            session,
            "ai.channelKnowledge.disable",
            vec![("channel_id", self.base.channel.clone())],
        ))
    }
    /// Only exact positive canonical readback can unlock the next distinct step.
    /// Absence of a change never grants permission to repeat the previous write.
    pub fn observe(&mut self, current: &Inventory) -> KnowledgeStage {
        match self.stage {
            KnowledgeStage::DisableUnconfirmed if current == &self.disabled() => {
                self.stage = KnowledgeStage::Disabled
            }
            KnowledgeStage::RemoveUnconfirmed if current == &self.removed() => {
                self.stage = KnowledgeStage::Complete
            }
            _ => {}
        }
        self.stage
    }
    /// Initial ACK flow or a later explicit Finish may call this once. Review
    /// itself only observes. Source omits keep_file_share for knowledge removal.
    pub fn begin_remove(
        &mut self,
        current: &Inventory,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Fault> {
        if self.stage != KnowledgeStage::Disabled || current != &self.disabled() {
            return Err(Fault::Changed);
        }
        self.stage = KnowledgeStage::RemoveUnconfirmed;
        Ok(client.rest_form(
            session,
            "conversations.removeTab",
            vec![
                ("channel_id", self.base.channel.clone()),
                ("id", self.id.clone()),
            ],
        ))
    }
}
#[cfg(test)]
mod tests;
