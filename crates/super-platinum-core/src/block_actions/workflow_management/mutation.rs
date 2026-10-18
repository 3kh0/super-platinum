use super::*;

/// Arguments for step writes are the source-prepared wire objects, not a visual
/// segment or an entire decorated step. 8dyy's segment conversion, form cleanup,
/// branching and context producers must run before constructing these commands.
#[derive(Clone, Debug, PartialEq)]
pub enum StepAction {
    Add {
        step: Value,
        input_parameters: Option<Value>,
        after_step_id: Option<String>,
    },
    Update {
        step: Value,
        input_parameters: Option<Value>,
    },
    Move {
        step_id: String,
        after_step_id: Option<String>,
    },
    Delete {
        step_id: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `trigger_update` is 1njB's prepared trigger arguments after RBJx's depth-
    /// limited conversion. Its identity must match this exact acquired editor.
    Metadata {
        name: String,
        description: String,
        icon_url: Option<String>,
        trigger_update: Option<Value>,
    },
    Published(bool),
    Delete,
    AddCollaborators(Vec<String>),
    RemoveCollaborators(Vec<String>),
    CopyPermission(String),
    RunPermission {
        permission_type: String,
        user_ids: Option<Vec<String>>,
        channel_ids: Option<Vec<String>>,
        team_ids: Option<Vec<String>>,
        org_ids: Option<Vec<String>>,
    },
    Step(StepAction),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowUp {
    TriggerInputSynchronization,
    StepInteractivity,
    StepUserContext,
    RestrictionRefresh,
    ConnectorEditPermissions,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Pending,
    InFlight,
    NotSent,
    Succeeded(Value),
    Rejected(Value),
    /// Transport crossed its send boundary but did not yield a definite result.
    /// This cannot become pending or authorize a dependent request.
    Unknown,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub(super) plan: RequestPlan,
    pub(super) prerequisite: Option<usize>,
    pub(super) outcome: Outcome,
}
impl Slot {
    pub fn plan(&self) -> &RequestPlan {
        &self.plan
    }
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
    pub fn prerequisite(&self) -> Option<usize> {
        self.prerequisite
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MutationPlan {
    original: Editor,
    command: Command,
    slots: Vec<Slot>,
    follow_ups: Vec<FollowUp>,
}
impl MutationPlan {
    pub fn new(editor: &Editor, command: Command) -> Result<Self, Error> {
        let workflow = editor.workflow_id();
        let mut slots = Vec::new();
        let mut follow_ups = Vec::new();
        let mut add = |method, reason, args, prerequisite| {
            slots.push(Slot {
                plan: RequestPlan::new(method, reason, args),
                prerequisite,
                outcome: Outcome::Pending,
            })
        };
        match &command {
            Command::Metadata {
                name,
                description,
                icon_url,
                trigger_update,
            } => {
                let changed_icon = icon_url.as_ref().filter(|icon| {
                    editor.raw_workflow()["icons"]["image_192"]
                        .as_str()
                        .unwrap_or("")
                        != icon.as_str()
                });
                if editor.raw_workflow()["title"].as_str().unwrap_or("") == name.as_str()
                    && editor.raw_workflow()["description"] == json!(description)
                    && changed_icon.is_none()
                {
                    return Err(invalid());
                }
                let mut args =
                    json!({"workflow_id":workflow,"title":name,"description":description});
                if let Some(icon) = changed_icon {
                    args["icon_url"] = json!(icon);
                }
                add(
                    "functions.workflows.update",
                    "update-hermes-workflow-metadata",
                    args,
                    None,
                );
                match (editor.trigger(), trigger_update) {
                    (None, None) => {}
                    (Some(trigger), Some(args)) => {
                        let object = args.as_object().ok_or_else(invalid)?;
                        // Closed source-produced trigger fields; no auth, method,
                        // reason or guessed workflow identity can be introduced.
                        if object.keys().any(|k| {
                            ![
                                "type",
                                "trigger_id",
                                "inputs",
                                "workflow",
                                "name",
                                "description",
                                "workflow_app_id",
                                "schedule",
                                "webhook",
                                "event",
                                "external",
                                "shortcut",
                                "service",
                            ]
                            .contains(&k.as_str())
                        }) || args["trigger_id"] != trigger["id"]
                            || args["workflow"] != json!(workflow)
                            || args["type"] != trigger["type"]
                            || !args["inputs"].is_object()
                            || args["workflow_app_id"] != editor.raw_workflow()["app_id"]
                        {
                            return Err(invalid());
                        }
                        // Source's empty name/description falls back on the
                        // prepared trigger; do not make that a second workflow.
                        if !name.is_empty() && args["name"] != json!(name)
                            || !description.is_empty() && args["description"] != json!(description)
                        {
                            return Err(invalid());
                        }
                        add(
                            "workflows.triggers.update",
                            "update-hermes-workflow-metadata",
                            args.clone(),
                            None,
                        );
                    }
                    _ => return Err(invalid()),
                }
            }
            Command::Published(published) => {
                add(
                    if *published {
                        "functions.workflows.publish"
                    } else {
                        "functions.workflows.unpublish"
                    },
                    if *published {
                        "publish-workflow"
                    } else {
                        "unpublish-workflow"
                    },
                    json!({"workflow_id":workflow}),
                    None,
                );
                if editor.trigger().is_some() {
                    follow_ups.push(FollowUp::TriggerInputSynchronization);
                }
            }
            Command::Delete => {
                let prerequisite = if let Some(trigger) = editor.trigger() {
                    add(
                        "workflows.triggers.delete",
                        "delete-hermes-workflow",
                        json!({"trigger_id":trigger["id"]}),
                        None,
                    );
                    Some(0)
                } else {
                    None
                };
                add(
                    "functions.workflows.delete",
                    "delete-workflow",
                    json!({"workflow_id":workflow}),
                    prerequisite,
                );
            }
            Command::AddCollaborators(ids) | Command::RemoveCollaborators(ids) => {
                valid_ids(ids)?;
                let adding = matches!(&command, Command::AddCollaborators(_));
                add(
                    if adding {
                        "functions.workflows.collaborators.add"
                    } else {
                        "functions.workflows.collaborators.remove"
                    },
                    if adding {
                        "add-hermes-workflow-collaborators"
                    } else {
                        "remove-hermes-workflow-collaborators"
                    },
                    json!({"workflow_id":workflow,"collaborator_ids":ids}),
                    None,
                );
            }
            Command::CopyPermission(permission) => {
                if !["everyone", "collaborators"].contains(&permission.as_str()) {
                    return Err(invalid());
                }
                add(
                    "functions.workflows.permissions.copy.set",
                    "set-copy-permission",
                    json!({"workflow_id":workflow,"permission_type":permission}),
                    None,
                );
            }
            Command::RunPermission {
                permission_type,
                user_ids,
                channel_ids,
                team_ids,
                org_ids,
            } => {
                let trigger = editor.trigger().ok_or_else(invalid)?;
                for ids in [user_ids, channel_ids, team_ids, org_ids]
                    .into_iter()
                    .flatten()
                {
                    valid_ids(ids)?;
                }
                if !["everyone", "app_collaborators", "named_entities"]
                    .contains(&permission_type.as_str())
                {
                    return Err(invalid());
                }
                let mut args =
                    json!({"trigger_id":trigger["id"],"permission_type":permission_type});
                if permission_type == "named_entities" {
                    for (key, ids) in [
                        ("user_ids", user_ids),
                        ("channel_ids", channel_ids),
                        ("team_ids", team_ids),
                        ("org_ids", org_ids),
                    ] {
                        if let Some(ids) = ids {
                            args[key] = json!(ids);
                        }
                    }
                } else if permission_type == "everyone" && (team_ids.is_some() || org_ids.is_some())
                {
                    args["permission_type"] = json!("named_entities");
                    if let Some(ids) = team_ids {
                        args["team_ids"] = json!(ids);
                    }
                    if let Some(ids) = org_ids {
                        args["org_ids"] = json!(ids);
                    }
                }
                add(
                    "workflows.triggers.permissions.set",
                    "set-trigger-run-permission",
                    args,
                    None,
                );
                follow_ups.push(FollowUp::RestrictionRefresh);
            }
            Command::Step(action) => {
                let (method, reason, args) = step_plan(editor, action)?;
                add(method, reason, args, None);
                follow_ups.extend([FollowUp::StepInteractivity, FollowUp::StepUserContext]);
                if matches!(action, StepAction::Add { .. } | StepAction::Delete { .. }) {
                    follow_ups.push(FollowUp::RestrictionRefresh);
                }
                if matches!(action, StepAction::Delete { .. }) {
                    follow_ups.push(FollowUp::ConnectorEditPermissions);
                }
            }
        }
        Ok(Self {
            original: editor.clone(),
            command,
            slots,
            follow_ups,
        })
    }
    pub fn original(&self) -> &Editor {
        &self.original
    }
    pub fn command(&self) -> &Command {
        &self.command
    }
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }
    /// Primary success is not completion when these source obligations remain.
    /// Their concrete plans require the actual primary response, not old state.
    pub fn follow_ups(&self) -> &[FollowUp] {
        &self.follow_ups
    }
    pub fn ready(&self) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.outcome == Outcome::Pending
                    && slot
                        .prerequisite
                        .is_none_or(|i| matches!(self.slots[i].outcome, Outcome::Succeeded(_)))
            })
            .map(|(i, _)| i)
            .collect()
    }
    pub fn mark_sent(&mut self, index: usize) -> Result<(), Error> {
        if !self.ready().contains(&index) {
            return Err(invalid());
        }
        self.slots[index].outcome = Outcome::InFlight;
        Ok(())
    }
    pub fn settle(&mut self, index: usize, outcome: Outcome) -> Result<(), Error> {
        let slot = self.slots.get_mut(index).ok_or_else(invalid)?;
        if slot.outcome != Outcome::InFlight {
            return Err(invalid());
        }
        match &outcome {
            Outcome::Succeeded(raw) => success(raw)?,
            Outcome::Rejected(raw) if raw.is_object() && raw["ok"] == json!(false) => {}
            Outcome::Unknown | Outcome::NotSent => {}
            _ => return Err(invalid()),
        }
        slot.outcome = outcome;
        Ok(())
    }
    pub fn primary_succeeded(&self) -> bool {
        self.slots
            .iter()
            .all(|s| matches!(s.outcome, Outcome::Succeeded(_)))
    }
}
fn valid_ids(ids: &[String]) -> Result<(), Error> {
    if ids.iter().all(|s| id(s)) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn step_plan(
    editor: &Editor,
    action: &StepAction,
) -> Result<(&'static str, &'static str, Value), Error> {
    let mut args = json!({"workflow_id":editor.workflow_id()});
    let find = |id: &str| find_step(editor.steps(), id);
    let after = |value: &Option<String>| -> Result<(), Error> {
        if value
            .as_deref()
            .is_none_or(|s| s.is_empty() || find(s).is_some())
        {
            Ok(())
        } else {
            Err(invalid())
        }
    };
    match action {
        StepAction::Add {
            step,
            input_parameters,
            ..
        }
        | StepAction::Update {
            step,
            input_parameters,
        } => {
            step.get("id")
                .and_then(Value::as_str)
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 255
                        && s.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                })
                .ok_or_else(invalid)?;
            identifier(step, "function_id")?;
            if !step["inputs"].is_object()
                || input_parameters.as_ref().is_some_and(|v| !v.is_object())
            {
                return Err(invalid());
            }
            let step_id = step["id"].as_str().expect("validated step");
            let adding = matches!(action, StepAction::Add { .. });
            if adding == find(step_id).is_some() {
                return Err(invalid());
            }
            args["step"] = step.clone();
            if let Some(parameters) = input_parameters {
                args["input_parameters"] = parameters.clone();
            }
            if let StepAction::Add { after_step_id, .. } = action {
                after(after_step_id)?;
                if let Some(value) = after_step_id {
                    args["after_step_id"] = json!(value);
                }
            }
            Ok((
                if adding {
                    "functions.workflows.steps.add"
                } else {
                    "functions.workflows.steps.update"
                },
                if adding || step["type"].as_str() == Some("case") {
                    "add-step-to-hermes-workflow"
                } else {
                    "update-hermes-workflow-step"
                },
                args,
            ))
        }
        StepAction::Move {
            step_id,
            after_step_id,
        } => {
            if find(step_id).is_none() || after_step_id.as_ref() == Some(step_id) {
                return Err(invalid());
            }
            after(after_step_id)?;
            args["step_id"] = json!(step_id);
            if let Some(value) = after_step_id {
                args["after_step_id"] = json!(value);
            }
            Ok((
                "functions.workflows.steps.move",
                "move-hermes-workflow-step",
                args,
            ))
        }
        StepAction::Delete { step_id } => {
            if find(step_id).is_none() {
                return Err(invalid());
            }
            args["step_id"] = json!(step_id);
            Ok((
                "functions.workflows.steps.delete",
                "remove-hermes-workflow-step",
                args,
            ))
        }
    }
}
fn find_step<'a>(steps: &'a [Value], id: &str) -> Option<&'a Value> {
    for step in steps {
        if step["id"].as_str() == Some(id) {
            return Some(step);
        }
        if let Some(children) = step["steps"].as_array() {
            if let Some(found) = find_step(children, id) {
                return Some(found);
            }
        }
    }
    None
}
