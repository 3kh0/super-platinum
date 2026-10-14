//! Journal each narrowly owned registration write before touching its target.
use super::{change, directory, model::*, platform};
use ::windows::{
    Win32::System::{Com::IPersistFile, Registry::*},
    core::{HSTRING, PCWSTR},
};
use std::{fs, io::Write};

fn intended(kind: Kind, values: &[(&str, &str)]) -> Contents {
    let mut values = values
        .iter()
        .map(|(name, value)| RegistryValue {
            name: (*name).into(),
            kind: REG_SZ.0,
            bytes: value
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect(),
        })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| a.name.cmp(&b.name));
    debug_assert!(kind != Kind::Shortcut);
    Contents::Registry(values)
}
fn plan(kind: Kind, expected: Contents) -> Result<(), String> {
    change(|ledger| {
        if ledger.phase != Phase::Acquiring
            || ledger
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == kind)
        {
            return Err("Cold registration step already attempted".into());
        }
        ledger.artifacts.push(Artifact {
            kind,
            expected,
            complete: false,
            created: false,
            removed: false,
        });
        Ok(())
    })
}
fn complete(kind: Kind) -> Result<(), String> {
    change(|ledger| {
        ledger
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.kind == kind)
            .ok_or("Missing cold write intent")?
            .complete = true;
        Ok(())
    })
}
fn created(kind: Kind) -> Result<(), String> {
    change(|ledger| {
        ledger
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.kind == kind)
            .ok_or("Missing cold write intent")?
            .created = true;
        Ok(())
    })
}
fn disown(kind: Kind) -> Result<(), String> {
    change(|ledger| {
        ledger
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.kind == kind)
            .ok_or("Missing cold write intent")?
            .removed = true;
        Ok(())
    })
}

pub(super) fn shortcut(persist: &IPersistFile) -> Result<(), String> {
    let staged = directory()?.join("prepared.lnk");
    if staged.exists() {
        return Err("Cold shortcut staging file already exists".into());
    }
    unsafe { persist.Save(&HSTRING::from(staged.as_os_str()), true) }.map_err(|e| e.to_string())?;
    let bytes = platform::read_bounded(&staged, 64 * 1024)?;
    // Record exact complete contents before publishing even the first byte.
    plan(Kind::Shortcut, Contents::File(bytes.clone()))?;
    let path = platform::shortcut()?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            if path.exists() {
                disown(Kind::Shortcut)?;
            }
            return Err(error.to_string());
        }
    };
    created(Kind::Shortcut)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    complete(Kind::Shortcut)
}

pub(super) fn registry(path: &str, values: &[(&str, &str)]) -> Result<(), String> {
    let kind = [Kind::Clsid, Kind::LocalServer, Kind::AppId]
        .into_iter()
        .find(|kind| platform::key(*kind).ok().as_deref() == Some(path))
        .ok_or("Unexpected cold registry path")?;
    plan(kind, intended(kind, values))?;
    let mut key = HKEY::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            Some(&mut disposition),
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    if disposition != REG_CREATED_NEW_KEY {
        unsafe {
            let _ = RegCloseKey(key);
        }
        disown(kind)?;
        return Err("Cold registry key appeared concurrently; preserved".into());
    }
    let result = (|| {
        created(kind)?;
        for (name, value) in values {
            let bytes = value
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            unsafe { RegSetValueExW(key, &HSTRING::from(*name), None, REG_SZ, Some(&bytes)) }
                .ok()
                .map_err(|e| e.to_string())?;
        }
        unsafe { RegFlushKey(key) }
            .ok()
            .map_err(|e| e.to_string())?;
        complete(kind)
    })();
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

pub(super) fn validate_expected(ledger: &Ledger) -> Result<(), String> {
    let guid = format!("{{{:?}}}", super::super::identity::FIXTURE_CLSID);
    let command = format!(
        "\"{}\" {}{}",
        ledger.executable.path,
        super::CHILD_ARG,
        ledger.nonce
    );
    for artifact in &ledger.artifacts {
        let valid = match artifact.kind {
            Kind::Shortcut => valid_shortcut(ledger, artifact)?,
            Kind::Clsid => artifact.expected == intended(Kind::Clsid, &[]),
            Kind::LocalServer => {
                artifact.expected == intended(Kind::LocalServer, &[("", &command)])
            }
            Kind::AppId => {
                artifact.expected
                    == intended(
                        Kind::AppId,
                        &[
                            ("DisplayName", "Super Platinum Fixture"),
                            ("CustomActivator", &guid),
                        ],
                    )
            }
        };
        if !valid {
            return Err("Cold ledger contains a foreign registration value".into());
        }
    }
    Ok(())
}
fn valid_shortcut(ledger: &Ledger, artifact: &Artifact) -> Result<bool, String> {
    use ::windows::{
        Win32::{
            Storage::EnhancedStorage::{
                PKEY_AppUserModel_ID, PKEY_AppUserModel_ToastActivatorCLSID,
            },
            System::Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance, STGM_READ,
                StructuredStorage::PropVariantToGUID,
            },
            UI::Shell::{IShellLinkW, PropertiesSystem::IPropertyStore, ShellLink},
        },
        core::Interface,
    };
    let path = platform::directory(&ledger.nonce)?.join("prepared.lnk");
    let bytes = platform::read_bounded(&path, 64 * 1024)?;
    if artifact.expected != Contents::File(bytes) {
        return Ok(false);
    }
    let _apartment = super::super::Apartment::new()?;
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| e.to_string())?;
    let persist: IPersistFile = link.cast().map_err(|e| e.to_string())?;
    unsafe { persist.Load(&HSTRING::from(path.as_os_str()), STGM_READ) }
        .map_err(|e| e.to_string())?;
    let store: IPropertyStore = link.cast().map_err(|e| e.to_string())?;
    let id = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }.map_err(|e| e.to_string())?;
    let id = ::windows::core::BSTR::try_from(&id)
        .map_err(|e| e.to_string())?
        .to_string();
    let class = unsafe { store.GetValue(&PKEY_AppUserModel_ToastActivatorCLSID) }
        .map_err(|e| e.to_string())?;
    let class = unsafe { PropVariantToGUID(&class) }.map_err(|e| e.to_string())?;
    let mut target = vec![0u16; 32768];
    let mut args = vec![0u16; 1024];
    let mut description = vec![0u16; 1024];
    unsafe { link.GetPath(&mut target, std::ptr::null_mut(), 0) }.map_err(|e| e.to_string())?;
    unsafe { link.GetArguments(&mut args) }.map_err(|e| e.to_string())?;
    unsafe { link.GetDescription(&mut description) }.map_err(|e| e.to_string())?;
    let description_length = description
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(description.len());
    let description =
        String::from_utf16(&description[..description_length]).map_err(|e| e.to_string())?;
    let length = target.iter().position(|c| *c == 0).unwrap_or(target.len());
    let target = String::from_utf16(&target[..length]).map_err(|e| e.to_string())?;
    Ok(id == super::super::identity::FIXTURE
        && class == super::super::identity::FIXTURE_CLSID
        && target == ledger.executable.path
        && description == format!("Super Platinum cold notification fixture {}", ledger.nonce)
        && args.first() == Some(&0))
}
pub(super) fn verify_complete(ledger: &Ledger) -> Result<(), String> {
    if ledger.artifacts.len() != 4
        || ledger
            .artifacts
            .iter()
            .any(|artifact| !artifact.complete || artifact.removed)
    {
        return Err("Cold registration handoff is incomplete".into());
    }
    for artifact in &ledger.artifacts {
        let actual = if artifact.kind == Kind::Shortcut {
            Some(Contents::File(platform::read_bounded(
                &platform::shortcut()?,
                64 * 1024,
            )?))
        } else {
            platform::registry_contents(artifact.kind)?
        };
        if actual.as_ref() != Some(&artifact.expected) {
            return Err("Cold registration changed before child launch; preserved".into());
        }
    }
    Ok(())
}
pub(super) fn remove(artifact: &Artifact) -> Result<(), String> {
    if artifact.kind == Kind::Shortcut {
        let path = platform::shortcut()?;
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        let actual = Contents::File(platform::read_bounded(&path, 64 * 1024)?);
        if !artifact.created {
            return Err("Shortcut creation outcome was not durably proved; preserved".into());
        }
        if !artifact.unchanged(&actual) {
            return Err("Cold shortcut changed; preserved for review".into());
        }
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        let Some(actual) = platform::registry_contents(artifact.kind)? else {
            return Ok(());
        };
        if !artifact.created {
            return Err("Registry creation outcome was not durably proved; preserved".into());
        }
        if !artifact.unchanged(&actual) {
            return Err("Cold registry values changed; preserved for review".into());
        }
        platform::delete_key(artifact.kind)
    }
}
