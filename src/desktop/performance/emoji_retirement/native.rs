//! Handles arrive only through the nonce/PID/creation-bound private readiness.
//! Mapping contains scalars only; event waits occur only in blocking producers.
use super::ledger;
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::{
        Memory::{FILE_MAP_READ, FILE_MAP_WRITE, MapViewOfFile},
        Threading::{GetCurrentThreadId, WaitForSingleObject},
    },
};

#[cfg(target_os = "windows")]
struct Owned(HANDLE);
#[cfg(target_os = "windows")]
impl Drop for Owned {
    fn drop(&mut self) {
        if unsafe { CloseHandle(self.0) }.is_err() {
            ledger::emit(ledger::event::FAILURE, 0, 0, 20);
        }
    }
}

/// Owns exactly the child duplicate. Its numeric representation can cross to a
/// blocking worker; no other code may use/close it after transfer.
pub(super) struct Event(usize);
impl Event {
    pub(super) unsafe fn from_transferred(value: usize) -> Result<Self, String> {
        if value == 0 || value == usize::MAX {
            return Err("Invalid event handle".into());
        }
        Ok(Self(value))
    }
    pub(super) fn wait(self, generation: u64) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let result = unsafe { WaitForSingleObject(HANDLE(self.0 as *mut _), 5000) };
            if result != WAIT_OBJECT_0 {
                ledger::emit(ledger::event::FAILURE, generation, 0, 21);
                return Err(format!("Event wait failed: {}", result.0));
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("Windows diagnostic required".into())
        }
    }
}
impl Drop for Event {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        drop(Owned(HANDLE(self.0 as *mut _)));
    }
}

/// Takes ownership of the duplicated mapping handle even if mapping fails.
pub(super) unsafe fn attach_mapping(
    handle: usize,
    nonce: [u64; 2],
    process: u64,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if handle == 0 || handle == usize::MAX {
            return Err("Invalid mapping handle".into());
        }
        let handle = Owned(HANDLE(handle as *mut _));
        let view = unsafe {
            MapViewOfFile(
                handle.0,
                FILE_MAP_READ | FILE_MAP_WRITE,
                0,
                0,
                ledger::BYTES,
            )
        };
        if view.Value.is_null() {
            return Err("MapViewOfFile failed".into());
        }
        // CloseHandle does not invalidate a mapped view. The diagnostic view has
        // process lifetime and is unmapped by process teardown; it owns no data.
        // The runner's own view/handle survives child exit and is always closed.
        let installed = unsafe {
            ledger::install(
                view.Value as usize,
                GetCurrentThreadId() as u64,
                nonce,
                process,
            )
        };
        if installed.is_err() {
            let _ = unsafe { windows::Win32::System::Memory::UnmapViewOfFile(view) };
        }
        installed
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (handle, nonce, process);
        Err("Windows diagnostic required".into())
    }
}

pub(super) fn parse_handle(value: &str) -> Result<usize, String> {
    if value.is_empty()
        || value.bytes().any(|c| !c.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err("Invalid decimal handle".into());
    }
    value
        .parse::<usize>()
        .ok()
        .filter(|&v| v != 0 && v != usize::MAX)
        .ok_or_else(|| "Handle outside pointer width".into())
}
pub(super) fn creation_filetime() -> Result<u64, String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{
            Foundation::FILETIME,
            System::Threading::{GetCurrentProcess, GetProcessTimes},
        };
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        }
        .map_err(|_| "Process identity unavailable")?;
        Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows diagnostic required".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_never_use_json_floats_or_wrapping_casts() {
        for value in ["", "0", "-1", "01", "1.0", "1e3", "18446744073709551616"] {
            assert!(parse_handle(value).is_err(), "{value}");
        }
        assert_eq!(parse_handle("32").unwrap(), 32);
    }
}
