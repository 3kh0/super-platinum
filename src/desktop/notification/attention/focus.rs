//! Read-only macOS 12+ Focus admission. Call only on a blocking worker.
//!
//! Source: macos-notification-state 2.0.2, lib/dnd/monterey-macos-dnd.mm,
//! npm gitHead 53d0fc4fc51f1b1133379c0944a44b28efb83c0d (also shipped by Slack).
//! https://github.com/felixrieseberg/macos-notification-state/tree/53d0fc4fc51f1b1133379c0944a44b28efb83c0d
//! The source treats every stored assertion as active, uses its first mode,
//! and does NOT interpret assertion expiry (an upstream TODO). Its fallback
//! schedule uses local minutes, ignores day/end-minute fields, and consults
//! mode "NONE" when there is no assertion. Preserve those rules rather than
//! inventing a newer private schema. Unlike its error=>false fallback, unknown
//! or malformed required input suppresses attention. These private formats may
//! change; this is not an Apple-supported Focus API or an expiry evaluator.

/* Algorithm adapted under the MIT License (MIT).
 * Copyright (c) 2017 Felix Rieseberg
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use serde_json::{Map, Value};
use std::{fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Unknown,
    Quiet,
    Available,
}

// Bounds both allocation and parsing. Oversized state is unknown, never clear.
const MAX_BYTES: u64 = 1024 * 1024;

pub(super) fn read(home: &Path, bundle_id: Option<&str>, local_minutes: u16) -> Status {
    if local_minutes >= 24 * 60 || !home.is_absolute() {
        return Status::Unknown;
    }
    let directory = home.join("Library/DoNotDisturb/DB");
    let Some(assertions) = read_json(&directory.join("Assertions.json")) else {
        return Status::Unknown;
    };
    let Ok((asserted, mode)) = assertion(&assertions) else {
        return Status::Unknown;
    };
    let active = if asserted {
        true
    } else {
        let Some(schedule) = read_json(&directory.join("ModeConfigurations.json")) else {
            return Status::Unknown;
        };
        match scheduled(&schedule, mode, local_minutes) {
            Ok(active) => active,
            Err(()) => return Status::Unknown,
        }
    };
    if !active {
        return Status::Available;
    }
    // A bare executable has no bundle identity and cannot inherit an exception.
    let Some(bundle) = bundle_id.filter(|id| !id.is_empty()) else {
        return Status::Quiet;
    };
    let Some(secure) = read_json(&directory.join("ModeConfigurationsSecure.json")) else {
        return Status::Unknown;
    };
    match allowed(&secure, mode, bundle) {
        Ok(true) => Status::Available,
        Ok(false) => Status::Quiet,
        Err(()) => Status::Unknown,
    }
}

fn read_json(path: &Path) -> Option<Value> {
    // Reject non-files before opening (including FIFOs). Recheck the opened file
    // and cap the actual read in case its length changes between these steps.
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let file = File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn data(value: &Value) -> Result<&Map<String, Value>, ()> {
    value
        .as_object()
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .and_then(Value::as_object)
        .ok_or(())
}

fn object<'a>(
    parent: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a Map<String, Value>>, ()> {
    match parent.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_object().map(Some).ok_or(()),
    }
}

fn assertion(value: &Value) -> Result<(bool, &str), ()> {
    let records = match data(value)?.get("storeAssertionRecords") {
        None | Some(Value::Null) => return Ok((false, "NONE")),
        Some(value) => value.as_array().ok_or(())?,
    };
    let Some(first) = records.first() else {
        return Ok((false, "NONE"));
    };
    let mode = first
        .as_object()
        .and_then(|v| v.get("assertionDetails"))
        .and_then(Value::as_object)
        .and_then(|v| v.get("assertionDetailsModeIdentifier"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or(())?;
    Ok((true, mode))
}

fn scheduled(value: &Value, mode: &str, now: u16) -> Result<bool, ()> {
    if now >= 24 * 60 {
        return Err(());
    }
    let Some(configs) = object(data(value)?, "modeConfigurations")? else {
        return Ok(false);
    };
    let Some(config) = object(configs, mode)? else {
        return Ok(false);
    };
    let Some(wrapper) = object(config, "triggers")? else {
        return Ok(false);
    };
    let triggers = match wrapper.get("triggers") {
        None | Some(Value::Null) => return Ok(false),
        Some(value) => value.as_array().ok_or(())?,
    };
    let mut active = false;
    for trigger in triggers {
        let trigger = trigger.as_object().ok_or(())?;
        let setting = match trigger.get("enabledSetting") {
            None | Some(Value::Null) => 0,
            Some(value) => value.as_i64().ok_or(())?,
        };
        if setting != 2 {
            continue;
        }
        let hour = |key| match trigger.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_u64().filter(|v| *v < 24).map(Some).ok_or(()),
        };
        let (Some(start), Some(end)) = (
            hour("timePeriodStartTimeHour")?,
            hour("timePeriodEndTimeHour")?,
        ) else {
            continue;
        };
        let minute = match trigger.get("timePeriodStartTimeMinute") {
            None | Some(Value::Null) => 0,
            Some(value) => value.as_u64().filter(|v| *v < 60).ok_or(())?,
        };
        let start = start * 60 + minute;
        let end = end * 60; // Upstream deliberately does not read end-minute.
        let now = u64::from(now);
        active |= (start < end && now >= start && now < end)
            || (end < start && (now >= start || now < end));
    }
    Ok(active)
}

fn allowed(value: &Value, mode: &str, bundle: &str) -> Result<bool, ()> {
    if bundle.is_empty() {
        return Ok(false);
    }
    let Some(configs) = object(data(value)?, "secureModeConfigurations")? else {
        return Ok(false);
    };
    let Some(config) = object(configs, mode)? else {
        return Ok(false);
    };
    let Some(secure) = object(config, "secureConfiguration")? else {
        return Ok(false);
    };
    let Some(apps) = object(secure, "allowedApplications")? else {
        return Ok(false);
    };
    // Upstream's child-process predicate is bundle CONTAINS allowlisted key.
    // Reject an empty key: it would grant every identity on malformed input.
    if apps.keys().any(String::is_empty) {
        return Err(());
    }
    Ok(apps.keys().any(|key| bundle.contains(key)))
}

#[cfg(test)]
#[path = "focus_tests.rs"]
mod tests;
