//! This is a stage of the original Share worker, under its existing custody lease.
//! Selecting a person performs no network work. The captured note is durable
//! before resolution; files.share remains behind its own durable Begin.
use super::*;
use super_platinum_core::file_sharing::drafts::Draft;

impl Worker {
    pub(super) async fn resolve_person(&mut self, saved: Draft) -> Result<Draft, Refusal> {
        let Some(user) = self.destination.id.strip_prefix("person:") else {
            return Ok(saved);
        };
        self.check()?;
        let storage = Storage::capture(&self.source.context)?;
        let key = self.source.context.key.clone();
        if storage
            .run(move |j| j.retry_at(&key.team, &key.user))
            .await?
            > now()
        {
            return Err(Refusal::Cooldown);
        }
        self.check()?;
        let person =
            new_direct::Person::capture(&self.signal.peek(), user).ok_or(Refusal::Destination)?;
        let revision = self.source_revision.ok_or(Refusal::Changed)?;
        let raw = self
            .source
            .context
            .file_info(&self.source.id)
            .await
            .map_err(|error| {
                self.read_error(&error);
                Refusal::Read
            })?;
        self.check()?;
        self.source.fresh(raw)?;
        let peer = self
            .source
            .context
            .peer_info(&person.id)
            .await
            .map_err(|error| {
                self.read_error(&error);
                Refusal::Read
            })?;
        self.check()?;
        if !person.fresh(&peer, &self.source.context.key.team) {
            return Err(Refusal::Destination);
        }
        if crate::file_events::changed_since(
            &self.signal.peek(),
            &saved.scope.team,
            &saved.scope.file,
            revision,
        ) {
            return Err(Refusal::Changed);
        }
        // Commit the full original draft + peer before the sole open. Existing
        // records (including crash/timeout) are read-only recovery, never replay.
        let original = saved.clone();
        let begin = storage
            .run(move |j| j.begin_direct_open(&original, peer, now()))
            .await?;
        use super_platinum_core::file_sharing::store::direct_open::Begin;
        let (record, created) = match begin {
            Begin::Created(record) => (record, true),
            Begin::Existing(record) => (record, false),
            Begin::Capacity => return Err(Refusal::Capacity),
        };
        self.check()?;
        if !person.current(&self.signal.peek()) {
            return Err(Refusal::Changed);
        }
        let record = if created {
            direct_recovery::reserve(&self.lease.custody, &storage, &record)?;
            // The original account/source/person fence above and reservation
            // are synchronous. No await precedes polling the single request.
            let result = self.source.context.open_direct(&person.id).await;
            if let Err(error) = &result {
                self.read_error(error);
            }
            // Captured-owner result custody precedes any UI/account check.
            direct_recovery::returned(&self.lease.custody, &record, result).await?
        } else {
            record
        };
        self.check()?;
        let id = if let Some(opened) = record
            .returned()
            .filter(|raw| new_direct::opened_id(raw).is_ok())
        {
            let id = new_direct::opened_id(opened)?;
            let c = &opened["channel"];
            if c.get("user")
                .is_some_and(|v| v.as_str() != Some(person.id.as_str()))
                || c.get("is_im").is_some_and(|v| v != &Value::Bool(true))
                || ["is_mpim", "is_channel", "is_archived", "is_ext_shared"]
                    .iter()
                    .any(|k| c.get(*k).is_some_and(|v| v != &Value::Bool(false)))
            {
                return Err(Refusal::Destination);
            }
            id
        } else {
            let listed = self.source.context.direct_lookup().await.map_err(|error| {
                self.read_error(&error);
                Refusal::Read
            })?;
            self.check()?;
            direct_recovery::candidate(&listed, &person.id)?
        };
        let raw = self
            .source
            .context
            .channel_info(&id)
            .await
            .map_err(|error| {
                self.read_error(&error);
                Refusal::Read
            })?;
        self.check()?;
        let channel = new_direct::verified_channel(&raw, &id, &person.id)?;
        // A retained historical open is not evidence that the DM is open now.
        // Uncertain lookup must provide a fresh explicit open fact, not a default.
        if !created && raw["channel"]["is_open"] != true {
            return Err(Refusal::RecipientResolution);
        }
        if raw["channel"]["is_open"] == true && record.resolved().is_none() {
            let expected = record.clone();
            let resolved_raw = raw.clone();
            storage
                .run(move |j| j.resolve_direct_open(&expected, resolved_raw))
                .await?;
            self.check()?;
        }
        // Keep the original event fence through DM resolution. The common
        // delivery path takes its own later revision only after this stage.
        if crate::file_events::changed_since(
            &self.signal.peek(),
            &saved.scope.team,
            &saved.scope.file,
            revision,
        ) {
            return Err(Refusal::Changed);
        }
        let capture = {
            // Copy the handle, retaining the same cell and exclusive write guard.
            let mut signal = self.signal;
            let mut shell = signal.write();
            if !self.current(&shell) || !person.current(&shell) {
                return Err(Refusal::Changed);
            }
            let destination = new_direct::remember(&mut shell, channel, &person)?;
            let recipient = recipients::capture(&shell, &id).ok_or(Refusal::Destination)?;
            shell.file_share.selected_recipient = Some((self.lease.id.clone(), id, recipient));
            shell
                .file_share
                .dialog
                .as_mut()
                .ok_or(Refusal::Changed)?
                .destination = Some(destination.clone());
            self.destination = destination;
            durable::capture::Capture::prepare(&mut shell, &self.lease.id)?
        };
        let queue = capture.queue.clone();
        let ticket = capture
            .persist(
                saved.content.note.clone(),
                Some(self.destination.clone()),
                false,
            )
            .await?;
        let outcome = ticket.wait().await;
        {
            let mut shell = self.signal.write();
            if self.source.context.current(&shell) {
                durable::apply_capture(&mut shell, &self.lease.id, &self.visit, &ticket, &outcome);
            }
        }
        match outcome {
            draft_io::Outcome::Saved(resolved) => {
                let _ = queue.acknowledge(&ticket);
                self.check()?;
                Ok(resolved)
            }
            draft_io::Outcome::Conflict(_) | draft_io::Outcome::Blocked(_) => Err(Refusal::Changed),
            _ => Err(Refusal::Storage),
        }
    }
}
