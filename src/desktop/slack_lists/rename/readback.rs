use super::*;
pub(super) fn checked(
    shell: &mut ShellState,
    guard: &Guard,
    result: Result<network::Metadata, Error>,
) -> bool {
    checked_with_records(shell, guard, result, None)
}
pub(super) fn checked_with_records(
    shell: &mut ShellState,
    guard: &Guard,
    result: Result<network::Metadata, Error>,
    records: Option<RecordsPage>,
) -> bool {
    if let Err(e) = &result {
        registry::record(&guard.key.owner, e);
    }
    let choice = with_registry(|r| {
        r.entries.get(&guard.key).is_some_and(|e| {
            e.serial == guard.serial
                && (e.target.is_board_choice()
                    || e.target.is_card_order()
                    || e.target.is_group_delete())
        })
    });
    let page_valid = records.as_ref().is_some_and(|page| {
        network::positive(&page.raw).is_ok()
            && super_platinum_core::slack::api::parse_list_records(
                page.raw.clone(),
                &guard.key.list,
            )
            .is_ok_and(|parsed| parsed == *page)
    });
    let result = if result.is_ok() && ((choice && !page_valid) || (!choice && records.is_some())) {
        Err(Error::Api("board_group_records_unverified".into()))
    } else {
        result
    };
    let current = guard.current(shell, Stage::Checking);
    let mut applied = false;
    let retained = with_registry(|r| {
        r.entries
            .get(&guard.key)
            .filter(|e| {
                e.serial == guard.serial && e.job == Some(guard.job) && e.stage == Stage::Checking
            })
            .cloned()
    });
    let Some(old) = retained else { return false };
    let transfer_child = refinement_save::readback::transfer_allowed(shell, &old);
    let mut stage = Stage::Uncertain;
    let mut proof = None;
    let mut available = None;
    let mut observed_field = None;
    let mut outcomes = old.group_delete_outcomes.map(group_delete::unconfirmed);
    let message = match result {
        Ok(loaded) if current => {
            let intent = old.intent.as_ref().unwrap();
            if !network::same_incarnation(intent.original(), &loaded.document) {
                "The List identity changed. The original request remains unconfirmed."
            } else {
                available = view_create::matching(shell, intent, &loaded);
                observed_field = text_create::matching(shell, intent, &loaded);
                if let Intent::GroupDelete(i) = intent {
                    outcomes = outcomes
                        .zip(records.as_ref())
                        .map(|(o, p)| group_delete::observe(o, i, &loaded.document, p));
                }
                stage = if available.is_some() {
                    Stage::Available
                } else if observed_field.is_some()
                    || match intent {
                        Intent::Refinement(_) => {
                            refinement_save::readback::observes(&old, &loaded.document)
                        }
                        Intent::GroupDelete(_) => outcomes.is_some_and(group_delete::observed),
                        Intent::CardOrder(i) => records
                            .as_ref()
                            .is_some_and(|p| i.observes_desired(&loaded.document, p)),
                        _ => intent.observes_desired(&loaded.document),
                    }
                {
                    Stage::Observed
                } else {
                    Stage::Conflict
                };
                let view = shell.slack_lists.view.as_mut().unwrap();
                view.title = loaded.document.title.clone();
                view.loaded.as_mut().unwrap().document = loaded.document;
                view.loaded.as_mut().unwrap().allow_lists = loaded.allow;
                view.loaded.as_mut().unwrap().personal_task_lists = Some(loaded.personal);
                if old.target.is_deletion() {
                    view.loaded.as_mut().unwrap().list_experiments = loaded.experiments;
                }
                if let Some(records) = records {
                    view.loaded.as_mut().unwrap().records = records;
                }
                view.revision = next_id();
                refinement_save::readback::release_pending(shell, &old);
                rebuild_rows(shell);
                refinement_save::readback::transfer(shell, &guard.key, old.serial, transfer_child);
                if old.target.is_grouping()
                    && stage == Stage::Observed
                    && !grouping::projection_current(shell, &old.target)
                {
                    stage = Stage::Conflict;
                }
                proof = owner::Review::capture(shell);
                applied = true;
                if stage == Stage::Available {
                    "A matching new view is now available. Opening it is navigation, not proof this request created it."
                } else if old.target.is_refinement() {
                    if stage == Stage::Observed {
                        "The current saved view matches these refinements. The preference result is independent."
                    } else {
                        "The current view differs from this Save. Earlier requests may still arrive; neither request will be repeated."
                    }
                } else if old.target.is_group_delete() {
                    if stage == Stage::Observed {
                        "The group option is absent and the matching items have the requested values. This does not prove which request changed them."
                    } else {
                        "The option and item results are shown separately. Review the current data. Earlier requests may still arrive; closing does not repeat or undo them."
                    }
                } else if old.target.is_card_order() {
                    if stage == Stage::Observed {
                        "The current item position and group match this move. This does not prove which request changed them."
                    } else {
                        "The returned items differ from this move. Review the current Board. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_board_choice() {
                    if stage == Stage::Observed {
                        "The current group option matches this change. Items were refreshed from Slack; this does not prove which request changed them."
                    } else {
                        "The returned group options differ. Review the current groups and items. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_reorder() {
                    if stage == Stage::Observed {
                        "The current shared view order matches this move. This does not prove which request changed it."
                    } else {
                        "The returned views differ from this move. Review the current order. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_deletion() {
                    if stage == Stage::Observed {
                        "The field is absent from the current schema and every view. This does not prove which request removed it."
                    } else {
                        "The returned schema differs from the requested deletion. Review the current fields. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_text_creation() {
                    if stage == Stage::Observed {
                        match old.field_choice.kind {
                            super_platinum_core::slack_lists::ColumnCreateKind::Text => {
                                "A matching Text field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::NumberDefault0 => {
                                "A matching Number field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Checkbox => {
                                "A matching Checkbox field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Select => {
                                "A matching Select field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Vote => {
                                "A matching Vote field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Rating => {
                                "A matching Rating field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::ChannelSingleDefault => {
                                "A matching Channel field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::Link => {
                                "A matching Link field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::PeopleSingleDefault => {
                                "A matching People field is now visible in this Table."
                            }
                            super_platinum_core::slack_lists::ColumnCreateKind::DateSingleDefault => {
                                "A matching Date field is now visible in this Table."
                            }
                        }
                    } else {
                        "No isolated matching field is visible in this Table. Review the returned fields. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.creation().is_some() {
                    "Review the returned views below. No isolated matching addition is available. The earlier create may still arrive; closing does not repeat or undo it."
                } else if old.target.is_filter() {
                    if stage == Stage::Observed {
                        "The current shared filter matches your choice. Stored values are unchanged."
                    } else {
                        "The returned view differs from your filter choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_grouping() {
                    if stage == Stage::Observed {
                        "The current shared grouping matches your choice. Stored values are unchanged."
                    } else {
                        "The returned grouping or its current projection differs from your choice. Review the actual view. Closing does not repeat or undo the earlier request."
                    }
                } else if old.target.is_sort() {
                    if stage == Stage::Observed {
                        "The current shared sort matches your choice. Stored values are unchanged."
                    } else {
                        "The returned view differs from your sort choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_visibility() {
                    if stage == Stage::Observed {
                        "The current field visibility matches your choice."
                    } else {
                        "The returned view differs from your visibility choice. Review the current state. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if old.target.is_precision() {
                    if stage == Stage::Observed {
                        "The current number format matches your choice. Stored numbers are unchanged."
                    } else {
                        "The returned schema differs from your number format choice. Review the current format below. The earlier request may still arrive; closing does not repeat or undo it."
                    }
                } else if stage == Stage::Observed {
                    "The current name matches your rename."
                } else {
                    "The returned schema differs from this rename. Review the current fields below. The earlier rename may still arrive. Closing this review does not repeat or undo it."
                }
            }
        }
        Ok(_) => "This List visit changed while checking. Reopen the List and check again.",
        Err(_) => {
            "Could not confirm the current schema. The earlier request will not be sent again."
        }
    };
    with_registry(|r| {
        let e = r.entries.get_mut(&guard.key).unwrap();
        e.stage = stage;
        e.group_delete_outcomes = outcomes;
        e.review = proof;
        e.available = available;
        e.observed_field = observed_field;
        e.message = message.into();
    });
    applied
}
