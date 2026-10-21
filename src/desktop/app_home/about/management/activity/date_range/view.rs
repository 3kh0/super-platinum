use super::*;
use crate::icons::{Icon, icon};

pub(in crate::app_home::about::management::activity) fn filter_button(
    a: &State,
    blocked: bool,
) -> Element {
    rsx! {
        button{id:"workflow-activity-filter",r#type:"button",disabled:blocked,
            "aria-expanded":a.calendar.filter_open.to_string(),"aria-controls":"workflow-activity-filter-panel",
            {icon(Icon::Filter,"icon")} "Filter"
        }
    }
}
pub(in crate::app_home::about::management::activity) fn picker(
    a: &State,
    visit: u64,
    document: u64,
    blocked: bool,
    reading: bool,
    loading_notice: bool,
) -> Element {
    let c = &a.calendar;
    let f = &a.filters;
    let label = c.label(f);
    rsx! {
        div{class:"workflow-date-range",
            div{class:"workflow-date-range-heading",
                label{r#for:"workflow-activity-date-range","Date Range"}
                if c.filter_open && loading_notice {
                    span{id:"workflow-activity-loading",role:"status","Loading activity…"}
                }
            }
            button{id:"workflow-activity-date-range",r#type:"button",disabled:blocked,"aria-label":"Date Range",
                "aria-disabled":(blocked||reading).to_string(),"data-calendar-read-busy":reading.to_string(),
                "aria-expanded":c.open.to_string(),"aria-haspopup":"dialog","aria-controls":"workflow-activity-calendar",
                "data-visit":visit.to_string(),"data-document":document.to_string(),
                "data-from":f.from.clone(),"data-to":f.to.clone(),"data-revision":f.revision.to_string(),
                "data-range-origin":f.date_origin.clone(),"data-default-from":f.default_from.clone(),"data-default-to":f.default_to.clone(),
                "data-calendar-epoch":c.epoch.to_string(),"data-calendar-ack":c.ack.to_string(),
                "data-calendar-open":c.open.to_string(),"data-calendar-dirty":c.dirty.to_string(),"data-calendar-pending":"false",
                "{label}"
            }
            if c.open { {calendar(c)} }
        }
    }
}
fn calendar(c: &Calendar) -> Element {
    let Some(month) = c.month else { return rsx! {} };
    let (year, m, _) = month.parts();
    let offset = (month.weekday() + 7 - c.week_start) % 7;
    let last = month.shift_month(1).0 - month.0;
    let cells = ((offset as i32 + last + 6) / 7) * 7;
    let first = Day(month.0 - offset as i32);
    let focus = c.focus.unwrap_or(month);
    let (range_start, range_end) = if let Some((start, end)) = c.start.zip(c.end) {
        (Some(start), Some(end))
    } else if let Some((start, hover)) = c.start.zip(c.hover.filter(|day| c.allowed(*day))) {
        (Some(start.min(hover)), Some(start.max(hover)))
    } else {
        (c.start, c.start)
    };
    let caption = format!("{} {year}", c.months[m as usize - 1]);
    rsx! {
        section{id:"workflow-activity-calendar",class:"workflow-date-calendar",role:"dialog","aria-label":"Choose date range",
            "data-month":format!("{year:04}-{m:02}"),"data-start":c.start.map(Day::iso).unwrap_or_default(),
            "data-end":c.end.map(Day::iso).unwrap_or_default(),"data-epoch":c.epoch.to_string(),"data-focus":focus.iso(),
            div{class:"workflow-calendar-heading",
                button{id:"workflow-activity-calendar-prev",r#type:"button",class:"icon-btn","aria-label":"Previous month","data-calendar-action":"prev",
                    {icon(Icon::ChevronRight,"icon workflow-calendar-previous")}
                }
                span{id:"workflow-activity-calendar-month","aria-live":"polite","{caption}"}
                button{id:"workflow-activity-calendar-next",r#type:"button",class:"icon-btn","aria-label":"Next month","data-calendar-action":"next",
                    {icon(Icon::ChevronRight,"icon")}
                }
            }
            div{role:"grid","aria-readonly":"true","aria-labelledby":"workflow-activity-calendar-month",class:"workflow-calendar-grid",
                div{role:"row",class:"workflow-calendar-week",
                    for index in 0..7usize {span{role:"columnheader",key:"{index}",{c.weekdays[(index+c.week_start as usize)%7].clone()}}}
                }
                for week in 0..cells/7{
                    div{role:"row",class:"workflow-calendar-week",
                        for index in 0..7{
                            {let day=Day(first.0+week*7+index);let selected=c.start==Some(day)||c.end==Some(day);
                             let in_range=range_start.zip(range_end).is_some_and(|(a,b)|a<=day&&day<=b);
                             let allowed=c.allowed(day);let (_,day_month,number)=day.parts();
                             rsx!{
                                div{role:"gridcell","aria-selected":selected.to_string(),key:"{day.iso()}",
                                    if day_month==m {
                                    button{r#type:"button","data-calendar-action":"day","data-date":day.iso(),
                                        "data-in-range":in_range.to_string(),"data-other-month":(day_month!=m).to_string(),
                                        "aria-label":number.to_string(),"aria-disabled":(!allowed).to_string(),
                                        "aria-current":if c.today==Some(day){"date"}else{"false"},
                                        tabindex:if day==focus{"0"}else{"-1"},
                                        title:if allowed{String::new()}else{"Choose a date within 90 days.".into()},
                                        "{number}"
                                    }
                                    }
                                }
                             }}
                        }
                    }
                }
            }
            p{class:"workflow-calendar-hint","Select a date range of up to 90 days."}
            div{class:"workflow-calendar-actions",
                button{id:"workflow-activity-calendar-cancel",r#type:"button","data-calendar-action":"cancel","Cancel"}
                button{id:"workflow-activity-calendar-save",r#type:"button","data-calendar-action":"save",disabled:c.start.is_none()||c.end.is_none(),"Save"}
            }
        }
    }
}
