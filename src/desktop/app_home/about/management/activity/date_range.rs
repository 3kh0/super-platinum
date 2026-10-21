//! Renderer-owned, transactional Activity calendar. Committed dates remain query facts.
use super::*;
mod date;
#[cfg(test)]
mod tests;
mod view;
use date::Day;
pub(super) use view::{filter_button, picker};

#[derive(Clone)]
pub(super) struct Calendar {
    pub(super) filter_open: bool,
    pub(super) open: bool,
    pub(super) dirty: bool,
    pub(super) epoch: u64,
    pub(super) ack: u64,
    pub(super) initialized: bool,
    pub(super) today: Option<Day>,
    pub(super) start: Option<Day>,
    pub(super) end: Option<Day>,
    pub(super) month: Option<Day>,
    pub(super) focus: Option<Day>,
    pub(super) hover: Option<Day>,
    pub(super) origin: &'static str,
    pub(super) week_start: u32,
    pub(super) months: Vec<String>,
    pub(super) short_months: Vec<String>,
    pub(super) weekdays: Vec<String>,
}
impl Default for Calendar {
    fn default() -> Self {
        Self {
            filter_open: false,
            open: false,
            dirty: false,
            epoch: next(),
            ack: 0,
            initialized: false,
            today: None,
            start: None,
            end: None,
            month: None,
            focus: None,
            hover: None,
            origin: "default",
            week_start: 0,
            months: Vec::new(),
            short_months: Vec::new(),
            weekdays: Vec::new(),
        }
    }
}
impl Calendar {
    pub(super) fn reset(&mut self) {
        self.filter_open = false;
        self.close(false);
        self.initialized = false;
    }
    fn close(&mut self, retain: bool) {
        self.open = false;
        self.hover = None;
        if !retain {
            self.start = None;
            self.end = None;
            self.dirty = false;
        }
        self.epoch = next();
    }
    fn defaults(&self) -> Option<(Day, Day)> {
        self.today.map(|day| (Day(day.0 - 90), day))
    }
    fn prepare(&mut self, filters: &Filters) {
        let Some((from, to)) = self.defaults() else {
            return;
        };
        self.start = Day::parse(&filters.from).or(Some(from));
        self.end = Day::parse(&filters.to).or(Some(to));
        self.month = self.start.map(Day::month);
        self.focus = self.start;
        self.origin = if (filters.from.is_empty() && filters.to.is_empty())
            || (filters.date_origin == "default"
                && filters.from == filters.default_from
                && filters.to == filters.default_to)
        {
            "default"
        } else {
            "selection"
        };
        self.dirty = false;
        self.initialized = true;
    }
    pub(super) fn allowed(&self, day: Day) -> bool {
        self.end.is_some() || self.start.is_none_or(|start| (day.0 - start.0).abs() < 90)
    }
    pub(super) fn label(&self, filters: &Filters) -> String {
        let pair = self.defaults().and_then(|(from, to)| {
            Day::parse(&filters.from)
                .or(Some(from))
                .zip(Day::parse(&filters.to).or(Some(to)))
        });
        if self.short_months.len() != 12 {
            return "Date Range".into();
        }
        pair.map(|(from, to)| {
            format!(
                "{} – {}",
                from.short(&self.short_months),
                to.short(&self.short_months)
            )
        })
        .unwrap_or_else(|| "Date Range".into())
    }
}

pub(in crate::app_home::about::management) fn snapshot(a: &State) -> Value {
    let c = &a.calendar;
    json!({"filterOpen":c.filter_open,"open":c.open,"dirty":c.dirty,"epoch":c.epoch.to_string(),
        "ack":c.ack.to_string(),"start":c.start.map(Day::iso),"end":c.end.map(Day::iso),
        "month":c.month.map(|d|d.iso()[..7].to_owned()),"origin":c.origin,
        "defaultFrom":c.defaults().map(|v|v.0.iso()),"defaultTo":c.today.map(Day::iso)})
}

#[derive(Deserialize)]
struct Event {
    action: String,
    seq: u64,
    epoch: String,
    revision: String,
    visit: String,
    document: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    buffers: Option<Buffers>,
}
#[derive(Deserialize)]
struct Buffers {
    status: String,
    limit: String,
    user: String,
    search: String,
}
#[derive(Deserialize)]
struct Init {
    today: String,
    months: Vec<String>,
    short_months: Vec<String>,
    weekdays: Vec<String>,
    week_start: u32,
}

pub(super) fn mount(mut state: Signal<ShellState>, visit: u64, initial: bool) {
    let Some(id) = document(&state.peek(), visit).map(|d| d.id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let encoded = json!({"visit":visit.to_string(),"document":id.to_string()});
        let mut eval = dioxus::document::eval(&format!(
            "{}\nawait workflowCalendarBridge({encoded},dioxus);",
            include_str!("date_range/bridge.js")
        ));
        let _release = crate::app_home::inputs::native::retain(eval);
        let Ok(Ok(init)) = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Init>()).await
        else {
            return;
        };
        let Some(today) = Day::parse(&init.today) else {
            return;
        };
        if init.months.len() != 12
            || init.short_months.len() != 12
            || init.weekdays.len() != 7
            || init.week_start > 6
        {
            return;
        }
        {
            let mut s = state.write();
            let Some(d) = document_mut(&mut s, visit).filter(|d| d.id == id && d.activity.active)
            else {
                return;
            };
            let c = &mut d.activity.calendar;
            c.today = Some(today);
            c.months = init.months;
            c.short_months = init.short_months;
            c.weekdays = init.weekdays;
            c.week_start = init.week_start;
            c.prepare(&d.activity.filters);
        }
        if initial {
            dioxus::core::queue_effect(move || load(state, visit, Page::First));
        }
        loop {
            if !document(&state.peek(), visit).is_some_and(|d| d.id == id && d.activity.active)
                || !visible(&state.peek(), visit)
            {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(500), eval.recv::<Event>()).await {
                Err(_) => continue,
                Ok(Err(_)) => break,
                Ok(Ok(event)) => {
                    if event.action == "closed" {
                        break;
                    }
                    let changed = apply(&mut state.write(), visit, id, &event);
                    if changed {
                        // The pending native event is retired only by the acknowledged
                        // render. Then the ordinary guarded reader captures the new pair.
                        let mut settled = dioxus::document::eval(&format!(
                            "dioxus.send(await (async()=>{{const b=document.getElementById('workflow-activity-date-range');for(let i=0;i<120;i++){{if(!b?.isConnected)break;if(b.dataset.calendarAck==='{}'&&b.dataset.calendarPending==='false')return true;await new Promise(requestAnimationFrame);}}return false;}})());",
                            event.seq
                        ));
                        let _keep = crate::app_home::inputs::native::retain(settled);
                        if matches!(
                            tokio::time::timeout(Duration::from_secs(3), settled.recv::<bool>())
                                .await,
                            Ok(Ok(true))
                        ) {
                            load(state, visit, Page::First);
                        }
                    }
                }
            }
        }
    });
}

fn apply(s: &mut ShellState, visit: u64, id: u64, e: &Event) -> bool {
    if !permitted(s, visit) {
        return false;
    }
    let reading = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .is_some_and(|h| h.read.is_some());
    let Some(d) = document_mut(s, visit).filter(|d| d.id == id && d.activity.active) else {
        return false;
    };
    let a = &mut d.activity;
    let c = &mut a.calendar;
    let current = e.visit == visit.to_string()
        && e.document == id.to_string()
        && e.epoch == c.epoch.to_string()
        && e.revision == a.filters.revision.to_string();
    if (e.action == "hover" && e.seq != c.ack)
        || (e.action != "hover" && e.seq != c.ack.saturating_add(1))
    {
        return false;
    }
    c.ack = c.ack.max(e.seq);
    if !current || reading {
        return false;
    }
    let mut changed = false;
    match e.action.as_str() {
        "filter" => {
            c.filter_open = !c.filter_open;
            if !c.filter_open {
                c.close(false);
                c.initialized = false;
            }
        }
        "open" | "focus" => {
            if !c.filter_open {
                return false;
            }
            if !c.initialized {
                c.prepare(&a.filters);
            }
            c.open = true;
            if e.action == "focus" {
                c.focus = c.start.or(c.month);
            }
        }
        "tab" => c.close(true),
        "cancel" | "dismiss" => c.close(false),
        "outer-close" => {
            c.filter_open = false;
            c.close(false);
            c.initialized = false;
        }
        "prev" | "next" if c.open => {
            c.month = c
                .month
                .map(|m| m.shift_month(if e.action == "prev" { -1 } else { 1 }));
            c.focus = c.month;
        }
        "day" if c.open => {
            let Some(day) = Day::parse(&e.value).filter(|day| c.allowed(*day)) else {
                return false;
            };
            if c.start.is_none() || c.end.is_some() {
                c.start = Some(day);
                c.end = None;
            } else {
                let first = c.start.unwrap();
                c.start = Some(first.min(day));
                c.end = Some(first.max(day));
            }
            c.focus = Some(day);
            c.dirty = true;
            c.origin = "selection";
            c.hover = None;
        }
        "move" if c.open => {
            let Some(day) = Day::parse(&e.value) else {
                return false;
            };
            c.focus = Some(day);
            c.month = Some(day.month());
        }
        "hover" if c.open => {
            c.hover = Day::parse(&e.value);
            return false;
        }
        "save" if c.open => {
            let Some((from, to)) = c.start.zip(c.end) else {
                return false;
            };
            let Some(buffers) = &e.buffers else {
                return false;
            };
            if buffers.status != a.filters.status
                || buffers.limit != a.filters.limit
                || buffers.user != a.filters.user
                || buffers.search != a.user_search
                || a.member_composing
            {
                return false;
            }
            let default = c.origin == "default"
                && (c.defaults() == Some((from, to))
                    || (a.filters.date_origin == "default"
                        && a.filters.default_from == from.iso()
                        && a.filters.default_to == to.iso()));
            if !default && to.0 - from.0 >= 90 {
                return false;
            }
            changed = a.filters.from != from.iso() || a.filters.to != to.iso();
            a.filters.from = from.iso();
            a.filters.to = to.iso();
            a.filters.date_origin = if default { "default" } else { "selection" }.into();
            a.filters.default_from = if default { from.iso() } else { String::new() };
            a.filters.default_to = if default { to.iso() } else { String::new() };
            if changed {
                a.filters.revision = next();
                a.member_generation = next();
                a.member_pending = None;
                a.member_results = None;
                a.page = None;
                a.previous.clear();
                a.detail = None;
                a.detail_row = None;
                a.detail_revision = next();
                a.detail_read = None;
                a.notice = None;
            }
            c.close(false);
        }
        _ => return false,
    }
    c.epoch = next();
    if changed {
        s.app_home.about.management.surface.as_mut().unwrap().read = None;
    }
    changed
}
