use super::*;
#[derive(Clone)]
pub(crate) struct Choice {
    owner: Arc<Picker>,
    prepared: Arc<Prepared>,
    custom: Option<Arc<custom::Ready>>,
    frame: u64,
    index: usize,
}
impl PartialEq for Choice {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &b.owner)
            && Arc::ptr_eq(&self.prepared, &b.prepared)
            && same_custom(&self.custom, &b.custom)
            && self.frame == b.frame
            && self.index == b.index
    }
}
impl Choice {
    pub(crate) fn id(&self) -> String {
        format!(
            "thread-root-first-add-choice-{}-{}-{}",
            self.owner.id, self.frame, self.index
        )
    }
    fn current(&self, s: &ShellState) -> bool {
        let st = &s.thread_feed_reads.reaction_toggle.first_add;
        let Some(session) = st.session.as_ref() else {
            return false;
        };
        self.owner.current(s)
            && self.owner.open.scope.caught_up(s)
            && session.frame == self.frame
            && session
                .ready
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &self.prepared))
            && session
                .matches
                .iter()
                .take(session.limit)
                .any(|i| *i == self.index)
            && self.prepared.loaded.current(s)
            && crate::emoji_catalog::current(s)
                .is_some_and(|p| Arc::ptr_eq(&p, &self.prepared.loaded))
            && if let Some(custom) = &self.custom {
                session.custom.mode
                    && session
                        .custom
                        .ready
                        .as_ref()
                        .is_some_and(|r| Arc::ptr_eq(r, custom))
                    && custom.current(s)
                    && self.index < custom.len()
            } else {
                !session.custom.mode && self.index < self.prepared.choices.len()
            }
    }
}
fn same_custom(a: &Option<Arc<custom::Ready>>, b: &Option<Arc<custom::Ready>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
pub(crate) struct PresentedChoice {
    pub name: String,
    pub glyph: String,
    pub image: Option<custom::ImageDisplay>,
    pub action: Option<Choice>,
}
pub(crate) struct Presentation {
    pub id: u64,
    pub frame: u64,
    pub query: String,
    pub active: usize,
    pub ready: bool,
    pub busy: bool,
    pub notice: String,
    pub can_retry: bool,
    pub custom_mode: bool,
    pub can_switch: bool,
    pub can_retry_custom: bool,
    pub choices: Vec<PresentedChoice>,
    pub total: usize,
}
pub(crate) fn presentation(s: &ShellState) -> Option<Presentation> {
    if s.overlay != Some(Overlay::ThreadRootFirstAdd) {
        return None;
    }
    let st = &s.thread_feed_reads.reaction_toggle.first_add;
    if layout(s) {
        return Some(Presentation {
            id: 0,
            frame: 0,
            query: String::new(),
            active: 0,
            ready: false,
            busy: false,
            notice: "Reaction choices are unavailable in this offline preview.".into(),
            can_retry: false,
            custom_mode: st.custom_layout,
            can_switch: false,
            can_retry_custom: false,
            total: 3,
            choices: (if st.custom_layout {
                vec![
                    ("custom_party", ":custom_party:"),
                    ("custom_wave", ":custom_wave:"),
                    ("custom_ship", ":custom_ship:"),
                ]
            } else {
                vec![("eyes", "👀"), ("heart", "❤️"), ("tada", "🎉")]
            })
            .into_iter()
            .map(|(n, g)| PresentedChoice {
                name: n.into(),
                glyph: g.into(),
                image: None,
                action: None,
            })
            .collect(),
        });
    }
    let session = st.session.as_ref()?;
    let p = &session.owner;
    let current = p.current(s);
    let caught = current && p.open.scope.caught_up(s);
    let loaded = session.ready.as_ref().is_some_and(|r| {
        r.loaded.current(s)
            && crate::emoji_catalog::current(s).is_some_and(|v| Arc::ptr_eq(&v, &r.loaded))
    });
    let blocked = crate::reaction_writes::registry(s).blocked(&p.open.scope.physical(&p.open.key));
    let custom_mode = session.custom.mode;
    let custom = custom::ready_for(session);
    let custom_current = custom.as_ref().is_some_and(|r| r.current(s));
    let busy = session.loading || custom_mode && session.custom.loading;
    let ready = caught && loaded && !blocked && !busy && (!custom_mode || custom_current);
    let choices = if current && loaded {
        let prepared = session.ready.as_ref().unwrap();
        session
            .matches
            .iter()
            .take(session.limit)
            .filter_map(|&index| {
                let (name, glyph, image) = if custom_mode {
                    let custom = custom.as_ref().filter(|_| custom_current)?;
                    let choice = custom.selection(index)?;
                    (
                        choice.name().to_owned(),
                        choice.fallback().to_owned(),
                        custom::image(p.clone(), custom.clone(), session.frame, index),
                    )
                } else {
                    let glyph = prepared.choices.get(index)?;
                    (glyph.name().to_owned(), glyph.glyph().to_owned(), None)
                };
                Some(PresentedChoice {
                    name,
                    glyph,
                    image,
                    action: Some(Choice {
                        owner: p.clone(),
                        prepared: prepared.clone(),
                        custom: custom.clone(),
                        frame: session.frame,
                        index,
                    }),
                })
            })
            .collect()
    } else {
        vec![]
    };
    let notice = if !current {
        "This conversation changed. Close and refresh Threads to choose a reaction.".into()
    } else if !caught {
        "Waiting for current conversation updates.".into()
    } else if blocked {
        "An earlier reaction is pending, unconfirmed or cooling down.".into()
    } else if session.ready.is_some() && !loaded {
        "Emoji changed. Close and refresh Threads to choose again.".into()
    } else if custom_mode && session.custom.ready.is_some() && !custom_current {
        "Workspace emoji changed. Close and refresh Threads to choose again.".into()
    } else if custom_mode {
        session.custom.notice.clone()
    } else {
        session.notice.clone()
    };
    Some(Presentation {
        id: p.id,
        frame: session.frame,
        query: if current {
            session.query.clone()
        } else {
            String::new()
        },
        active: session.cursor.min(choices.len().saturating_sub(1)),
        ready,
        busy,
        notice,
        can_retry: current && caught && !blocked && !session.loading && session.ready.is_none(),
        custom_mode,
        can_switch: current && caught && loaded && !session.loading,
        can_retry_custom: current
            && caught
            && loaded
            && !blocked
            && custom_mode
            && !session.custom.loading
            && session.custom.ready.is_none(),
        choices,
        total: if current { session.matches.len() } else { 0 },
    })
}
pub(super) fn bump(session: &mut Session) -> bool {
    let Some(next) = session.frame.checked_add(1) else {
        session.owner.alive.store(false, Ordering::Release);
        session.ready = None;
        session.custom.ready = None;
        return false;
    };
    session.frame = next;
    true
}
pub(super) fn matching(prepared: &Prepared, query: &str) -> Vec<usize> {
    let choices = &prepared.choices;
    // The existing checked query frame binds every raw literal change, even
    // when two different literals normalize to the same exact alias.
    if let Some(alias) = AliasQuery::parse(query) {
        if let Some((index, _)) = choices
            .iter()
            .enumerate()
            .find(|(_, c)| c.alias_matches(&alias))
        {
            return vec![index];
        }
    }
    if let Some(matches) = prepared.name_model.query(query) {
        return matches;
    }
    let query = query.trim().trim_matches(':').to_lowercase();
    let mut found = choices
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_canonical())
        .filter(|(_, c)| {
            query.is_empty()
                || c.base().contains(&query)
                || c.name().contains(&query)
                || c.glyph() == query
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if !query.is_empty() {
        found.sort_by_key(|&i| {
            if choices[i].base() == query
                || choices[i].name() == query
                || choices[i].glyph() == query
            {
                0
            } else if choices[i].base().starts_with(&query) {
                1
            } else {
                2
            }
        });
    }
    found
}
pub(super) struct QueryWork {
    owner: Arc<Picker>,
    prepared: Arc<Prepared>,
    custom: Option<Arc<custom::Ready>>,
    mode: bool,
    literal: String,
    frame: u64,
}
impl QueryWork {
    pub(super) fn for_session(session: &Session) -> Option<Self> {
        let mode = session.custom.mode;
        let custom = if mode {
            Some(custom::ready_for(session)?)
        } else {
            None
        };
        Some(Self {
            owner: session.owner.clone(),
            prepared: session.ready.clone()?,
            custom,
            mode,
            literal: session.query.clone(),
            frame: session.frame,
        })
    }
    pub(super) fn compute(&self) -> Vec<usize> {
        if let Some(custom) = &self.custom {
            custom.matching(&self.literal)
        } else {
            matching(&self.prepared, &self.literal)
        }
    }
}
pub(super) fn capture_query(
    mut state: Signal<ShellState>,
    id: u64,
    text: String,
) -> Option<QueryWork> {
    let mut s = state.write();
    let p = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_ref()
        .filter(|p| p.owner.id == id)
        .map(|p| p.owner.clone())?;
    if !p.current(&s) {
        return None;
    }
    let session = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_mut()?;
    if session.query == text || !bump(session) {
        return None;
    }
    if text.len() > 4096 {
        if session.custom.mode {
            session.custom.notice = "Search text is too long.".into();
        } else {
            session.notice = "Search text is too long.".into();
        }
        return None;
    }
    session.query = text;
    session.cursor = 0;
    session.limit = 120;
    session.matches.clear();
    QueryWork::for_session(session)
}
pub(super) fn publish_query(mut state: Signal<ShellState>, work: QueryWork, matches: Vec<usize>) {
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if !work.owner.current(&s)
        || !work.prepared.loaded.current(&s)
        || !crate::emoji_catalog::current(&s)
            .is_some_and(|c| Arc::ptr_eq(&c, &work.prepared.loaded))
        || work.custom.as_ref().is_some_and(|r| !r.data_current(&s))
    {
        return;
    }
    let Some(session) = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_mut()
        .filter(|p| {
            p.frame == work.frame
                && p.query == work.literal
                && p.custom.mode == work.mode
                && (!work.mode || same_custom(&p.custom.ready, &work.custom))
                && p.ready
                    .as_ref()
                    .is_some_and(|r| Arc::ptr_eq(r, &work.prepared))
        })
    else {
        return;
    };
    session.matches = matches;
}
pub(crate) fn set_query(state: Signal<ShellState>, id: u64, text: String) {
    let Some(work) = capture_query(state, id, text) else {
        return;
    };
    // Bounded pure computation runs outside Shell write/ingress locks.
    let matches = work.compute();
    publish_query(state, work, matches);
}
pub(crate) fn move_cursor(mut state: Signal<ShellState>, id: u64, frame: u64, delta: isize) {
    let mut s = state.write();
    let Some(p) = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_ref()
        .filter(|p| p.owner.id == id && p.frame == frame)
        .map(|p| p.owner.clone())
    else {
        return;
    };
    if !p.current(&s) {
        return;
    }
    let session = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_mut()
        .unwrap();
    let count = session.matches.len().min(session.limit);
    session.cursor = session
        .cursor
        .saturating_add_signed(delta)
        .min(count.saturating_sub(1));
}
pub(crate) fn show_more(mut state: Signal<ShellState>, id: u64, frame: u64) {
    let mut s = state.write();
    let Some(p) = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_ref()
        .filter(|p| p.owner.id == id && p.frame == frame)
        .map(|p| p.owner.clone())
    else {
        return;
    };
    if !p.current(&s) {
        return;
    }
    let session = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_mut()
        .unwrap();
    if bump(session) {
        session.limit = session.limit.saturating_add(120).min(session.matches.len());
    }
}
pub(crate) fn choose_active(state: Signal<ShellState>, id: u64, frame: u64) {
    let choice = {
        let s = state.peek();
        let Some(session) = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .filter(|p| p.owner.id == id && p.frame == frame)
        else {
            return;
        };
        let Some(prepared) = session.ready.as_ref() else {
            return;
        };
        let Some(&index) = session
            .matches
            .iter()
            .take(session.limit)
            .nth(session.cursor)
        else {
            return;
        };
        Choice {
            owner: session.owner.clone(),
            prepared: prepared.clone(),
            custom: custom::ready_for(session),
            frame,
            index,
        }
    };
    choose(state, choice);
}
pub(crate) fn choose(mut state: Signal<ShellState>, choice: Choice) {
    let work = {
        let mut s = state.write();
        if !choice.current(&s) {
            return;
        }
        let selected = if let Some(custom) = &choice.custom {
            let Some(selected) = custom.selection(choice.index) else {
                return;
            };
            Selected::Custom(selected)
        } else {
            let Some(selected) = choice.prepared.choices.get(choice.index) else {
                return;
            };
            selected.clone()
        };
        let p = &choice.owner;
        let ledger = crate::reaction_writes::registry(&s);
        let Some(reservation) = ledger.claim(&p.open.scope.physical(&p.open.key)) else {
            return;
        };
        let Some(id) = super::super::next() else {
            reservation.finish(crate::reaction_writes::Receipt {
                id: 0,
                name: String::new(),
                adding: false,
                outcome: Outcome::NotSent,
            });
            return;
        };
        if p.chosen
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            reservation.finish(crate::reaction_writes::Receipt {
                id: 0,
                name: String::new(),
                adding: false,
                outcome: Outcome::NotSent,
            });
            return;
        }
        let intent = Arc::new(Intent {
            id,
            open: p.open.clone(),
            prepared: choice.prepared.clone(),
            selected,
            ledger,
            phase: std::sync::atomic::AtomicU8::new(0),
            notice: Mutex::new("Checking the selected reaction…".into()),
        });
        // Intent owns the original source before this expected UI departure.
        // A generic Close/use_drop can never cancel or recreate this intent.
        p.alive.store(false, Ordering::Release);
        s.thread_feed_reads.reaction_toggle.first_add.session = None;
        s.thread_feed_reads.reaction_toggle.first_add.ui_epoch = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .ui_epoch
            .saturating_add(1);
        s.thread_feed_reads.reaction_toggle.first_add.active = Some(intent.clone());
        s.overlay = None;
        (intent, reservation)
    };
    dioxus::core::spawn_forever(run::toggle(state, work.0, work.1));
}
pub(crate) fn close(mut state: Signal<ShellState>, id: u64) {
    // Component retirement can race Signal disposal. Cleanup is best-effort
    // and exact-owner only; it never cancels an already-issued intent.
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if layout(&s) && id == 0 {
        close_shell(&mut s);
        return;
    }
    if s.thread_feed_reads
        .reaction_toggle
        .first_add
        .session
        .as_ref()
        .is_some_and(|p| p.owner.id == id)
    {
        close_shell(&mut s);
    }
}
pub(crate) fn close_shell(s: &mut ShellState) {
    if let Some(session) = s.thread_feed_reads.reaction_toggle.first_add.session.take() {
        session.owner.alive.store(false, Ordering::Release);
    }
    s.thread_feed_reads.reaction_toggle.first_add.ui_epoch = s
        .thread_feed_reads
        .reaction_toggle
        .first_add
        .ui_epoch
        .saturating_add(1);
    if s.overlay == Some(Overlay::ThreadRootFirstAdd) {
        s.overlay = None;
    }
}
pub(crate) fn retry(mut state: Signal<ShellState>, id: u64) {
    let work = {
        let mut s = state.write();
        let Some(p) = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()
            .filter(|p| p.owner.id == id && !p.loading && p.ready.is_none())
            .map(|p| p.owner.clone())
        else {
            return;
        };
        if !p.current(&s)
            || !p.open.scope.caught_up(&s)
            || crate::reaction_writes::registry(&s).blocked(&p.open.scope.physical(&p.open.key))
        {
            return;
        }
        let Ok(old) = p
            .attempt
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        else {
            return;
        };
        let session = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_mut()
            .unwrap();
        if !bump(session) {
            return;
        }
        session.loading = true;
        session.notice = "Checking this message and loading emoji…".into();
        (p, old + 1)
    };
    dioxus::core::spawn_forever(run::prepare(state, work.0, work.1));
}
