//! B 9Y08 -> S fUG7/GuNR/Nmg4/GS5V/NIxh. Locale date patterns come
//! from BIeE, month/ordinal data from the captured Dayjs locale modules in P.
use super::TimestampContext;
use chrono::{DateTime, Datelike, FixedOffset, Timelike};

pub(super) fn format(date: DateTime<FixedOffset>, context: &TimestampContext) -> String {
    // The app's surrounding UI copy is English. Date/time tokens follow the
    // workspace locale; this is the source message's untranslated join string.
    format!(
        "{} at {}",
        calendar_date(date, context),
        time(date, &context.locale, context.time24)
    )
}

fn calendar_date(date: DateTime<FixedOffset>, context: &TimestampContext) -> String {
    let locale = context.locale.as_str();
    let now = context.calendar.now();
    let days = date
        .date_naive()
        .signed_duration_since(now.date_naive())
        .num_days();
    if (-1..=1).contains(&days) {
        let names = match locale {
            "de-DE" => ["Gestern", "Heute", "Morgen"],
            "es-ES" | "es-LA" => ["Ayer", "Hoy", "Mañana"],
            "fr-FR" => ["Hier", "Aujourd’hui", "Demain"],
            "it-IT" => ["Ieri", "Oggi", "Domani"],
            "ja-JP" => ["昨日", "今日", "明日"],
            "ko-KR" => ["어제", "오늘", "내일"],
            "pt-BR" => ["Ontem", "Hoje", "Amanhã"],
            "zh-CN" | "zh-TW" => ["昨天", "今天", "明天"],
            _ => ["Yesterday", "Today", "Tomorrow"],
        };
        return names[(days + 1) as usize].into();
    }
    let omit_year = date.year() == now.year()
        || (date.year() < now.year() && now.timestamp() - date.timestamp() <= 31 * 86400);
    let month = month(date.month0() as usize, locale);
    let day = date.day();
    let year = date.year();
    let ordinal = match locale {
        "en-US" => {
            let suffix = match day % 100 {
                11..=13 => "th",
                _ => match day % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                },
            };
            format!("{day}{suffix}")
        }
        "de-DE" => format!("{day}."),
        "fr-FR" if day == 1 => "1er".into(),
        "it-IT" | "pt-BR" if day == 1 => "1º".into(),
        "ja-JP" => format!("{day}日"),
        "ko-KR" => format!("{day}일"),
        _ => day.to_string(),
    };
    match locale {
        "en-US" => {
            if omit_year {
                format!("{month} {ordinal}")
            } else {
                format!("{month} {ordinal}, {year}")
            }
        }
        "es-LA" => {
            if omit_year {
                format!("{day} de {month}")
            } else {
                format!("{day} de {month} de {year}")
            }
        }
        "ja-JP" => {
            if omit_year {
                format!("{month}{ordinal}")
            } else {
                format!("{year}年{month}{ordinal}")
            }
        }
        "ko-KR" => {
            if omit_year {
                format!("{month} {ordinal}")
            } else {
                format!("{year}년 {month} {ordinal}")
            }
        }
        "zh-CN" | "zh-TW" => {
            if omit_year {
                format!("{} 月 {day} 日", date.month())
            } else {
                format!("{year} 年 {} 月 {day} 日", date.month())
            }
        }
        "pt-BR" => {
            if omit_year {
                format!("{ordinal} {month}")
            } else {
                format!("{ordinal} {month}, {year}")
            }
        }
        _ => {
            if omit_year {
                format!("{ordinal} {month}")
            } else {
                format!("{ordinal} {month} {year}")
            }
        }
    }
}

fn month(index: usize, locale: &str) -> &'static str {
    let months = match locale {
        "de-DE" => [
            "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.",
            "Dez.",
        ],
        "es-ES" | "es-LA" => [
            "ene.", "feb.", "mar.", "abr.", "may.", "jun.", "jul.", "ago.", "sep.", "oct.", "nov.",
            "dic.",
        ],
        "fr-FR" => [
            "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
            "nov.", "déc.",
        ],
        "it-IT" => [
            "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
        ],
        "pt-BR" => [
            "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
        ],
        "ja-JP" => [
            "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
        ],
        "ko-KR" => [
            "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
        ],
        _ => [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
    };
    months[index]
}

fn time(date: DateTime<FixedOffset>, locale: &str, time24: bool) -> String {
    let h = date.hour();
    let m = date.minute();
    let s = date.second();
    match locale {
        "de-DE" => format!("{h:02}:{m:02}:{s:02} Uhr"),
        "fr-FR" => {
            if s != 0 {
                format!("{h} h {m:02} min {s:02} s")
            } else if m != 0 {
                format!("{h} h {m:02}")
            } else {
                format!("{h} h")
            }
        }
        "pt-BR" => {
            if s != 0 {
                format!("{h}h{m:02}min{s:02}s")
            } else if m != 0 {
                format!("{h}h{m:02}")
            } else {
                format!("{h}h")
            }
        }
        "it-IT" | "es-LA" => format!("{h}:{m:02}:{s:02}"),
        "es-ES" | "ja-JP" => format!("{h:02}:{m:02}:{s:02}"),
        _ if time24 => format!("{h:02}:{m:02}:{s:02}"),
        _ => {
            let hour = if h % 12 == 0 { 12 } else { h % 12 };
            let clock = format!("{hour}:{m:02}:{s:02}");
            match locale {
                "ko-KR" => format!("{} {clock}", if h < 12 { "오전" } else { "오후" }),
                "zh-CN" | "zh-TW" => {
                    let hm = h * 100 + m;
                    let meridiem = match hm {
                        0..600 => "凌晨",
                        600..900 => "早上",
                        900..1130 => "上午",
                        1130..1230 => "中午",
                        1230..1800 => "下午",
                        _ => "晚上",
                    };
                    format!("{meridiem} {clock}")
                }
                _ => format!("{clock} {}", if h < 12 { "AM" } else { "PM" }),
            }
        }
    }
}
