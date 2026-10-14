//! Main-thread taskbar overlay with observable native failures and recovery.
use dioxus::desktop::{DesktopContext, tao::platform::windows::WindowExtWindows};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{
            Shell::{
                DefSubclassProc, ITaskbarList3, RemoveWindowSubclass, SetWindowSubclass,
                TaskbarList,
            },
            WindowsAndMessaging::{HICON, RegisterWindowMessageW, SendMessageW, WM_NCDESTROY},
        },
    },
    core::{PCWSTR, w},
};
#[path = "windows_icon.rs"]
mod icon;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub(super) struct Receipt {
    pub label: Option<String>,
    pub desired_label: Option<String>,
    pub attempts: usize,
    pub successes: usize,
    pub recreations: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub hwnd: isize,
    pub native: Receipt,
}

/// Same lazy native owner used by the production hook. Snapshot reads neither
/// create a sink nor write an overlay, and expose no mutable native handle.
pub(super) struct NativeWriter {
    desktop: DesktopContext,
    sink: Option<Rc<Sink>>,
}
impl NativeWriter {
    pub(super) fn new(desktop: DesktopContext) -> Self {
        Self {
            desktop,
            sink: None,
        }
    }
}
impl super::Writer for NativeWriter {
    fn write(&mut self, label: Option<String>) -> Result<(), String> {
        if self.sink.is_none() {
            self.sink = Some(Sink::new(&self.desktop)?);
        }
        self.sink.as_ref().unwrap().write(label)
    }
    fn snapshot(&self) -> Option<Snapshot> {
        self.sink.as_ref().map(|sink| sink.snapshot())
    }
}
struct State {
    icon: Option<icon::Icon>,
    icon_ready: bool,
    receipt: Receipt,
}
pub(super) struct Sink {
    hwnd: Cell<HWND>,
    hook_data: Cell<usize>,
    message: u32,
    size: usize,
    pending_recreation: Cell<bool>,
    state: RefCell<State>,
}

impl Sink {
    pub(super) fn new(desktop: &DesktopContext) -> Result<Rc<Self>, String> {
        let message = unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) };
        if message == 0 {
            return Err("RegisterWindowMessageW failed".into());
        }
        let sink = Rc::new(Self {
            hwnd: Cell::new(HWND(desktop.window.hwnd() as *mut _)),
            hook_data: Cell::new(0),
            message,
            size: (16.0 * desktop.window.scale_factor())
                .round()
                .clamp(16.0, 64.0) as usize,
            pending_recreation: Cell::new(false),
            state: RefCell::new(State {
                icon: None,
                icon_ready: true,
                receipt: Receipt::default(),
            }),
        });
        // The callback slot survives a failed removal, but its Weak never keeps
        // the controller alive. Each callback upgrades before accessing Sink.
        let pointer = Box::into_raw(Box::new(Rc::downgrade(&sink))) as usize;
        let registered =
            unsafe { SetWindowSubclass(sink.hwnd.get(), Some(subclass), pointer, pointer) }.ok();
        if let Err(error) = registered {
            unsafe {
                drop(Box::from_raw(pointer as *mut Weak<Sink>));
            }
            return Err(format!("SetWindowSubclass: {error}"));
        }
        sink.hook_data.set(pointer);
        Ok(sink)
    }

    pub(super) fn write(&self, label: Option<String>) -> Result<(), String> {
        let result = {
            let mut state = self.state.borrow_mut();
            // Desired ownership changes before any fallible raster/API call.
            // Explorer recovery must never resurrect a retired account's icon.
            state.receipt.desired_label = label;
            state.icon = None;
            state.icon_ready = false;
            self.apply(&mut state)
        };
        self.flush_recreation();
        result
    }

    fn apply(&self, state: &mut State) -> Result<(), String> {
        if self.hwnd.get().0.is_null() {
            let error = "Taskbar window was retired".to_owned();
            state.receipt.last_error = Some(error.clone());
            return Err(error);
        }
        if !state.icon_ready {
            match state
                .receipt
                .desired_label
                .as_deref()
                .map(|label| icon::create(label, self.size))
                .transpose()
            {
                Ok(icon) => {
                    state.icon = icon;
                    state.icon_ready = true;
                }
                Err(error) => {
                    state.receipt.last_error = Some(error.clone());
                    return Err(error);
                }
            }
        }
        state.receipt.attempts += 1;
        let description: Vec<u16> = description(state.receipt.desired_label.as_deref())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let result = unsafe {
            let taskbar: windows::core::Result<ITaskbarList3> =
                CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER);
            taskbar.and_then(|taskbar| {
                taskbar.HrInit()?;
                taskbar.SetOverlayIcon(
                    self.hwnd.get(),
                    state
                        .icon
                        .as_ref()
                        .map(|icon| icon.0)
                        .unwrap_or(HICON::default()),
                    PCWSTR(description.as_ptr()),
                )
            })
        };
        match result {
            Ok(()) => {
                state.receipt.successes += 1;
                state.receipt.label = state.receipt.desired_label.clone();
                state.receipt.last_error = None;
                Ok(())
            }
            Err(error) => {
                let error = format!(
                    "SetOverlayIcon HRESULT {:#010x}: {error}",
                    error.code().0 as u32
                );
                state.receipt.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn flush_recreation(&self) {
        if !self.pending_recreation.get() {
            return;
        }
        if let Ok(mut state) = self.state.try_borrow_mut() {
            self.pending_recreation.set(false);
            state.receipt.recreations += 1;
            if let Err(error) = self.apply(&mut state) {
                eprintln!("super-platinum: taskbar recreation: {error}");
            }
        }
    }

    pub(super) fn receipt(&self) -> Receipt {
        self.state.borrow().receipt.clone()
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            hwnd: self.hwnd(),
            native: self.receipt(),
        }
    }
    pub(super) fn hwnd(&self) -> isize {
        self.hwnd.get().0 as isize
    }
    pub(super) fn simulate_recreation(&self) {
        // Fixture-only caller: same message callback, without restarting Explorer.
        unsafe {
            SendMessageW(self.hwnd.get(), self.message, None, None);
        }
    }
}

fn description(label: Option<&str>) -> String {
    match label {
        None => "No unread badge".into(),
        Some("•") => "Unread messages".into(),
        Some(count) => format!("{count} unread notifications"),
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        let hwnd = self.hwnd.get();
        if !hwnd.0.is_null() {
            let pointer = self.hook_data.replace(0);
            // Unregister before state/icon destruction. WM_NCDESTROY also retires it.
            if pointer != 0 {
                if unsafe { RemoveWindowSubclass(hwnd, Some(subclass), pointer) }.as_bool() {
                    unsafe {
                        drop(Box::from_raw(pointer as *mut Weak<Sink>));
                    }
                } else {
                    // Keep the weak callback slot valid until WM_NCDESTROY.
                    // If another party removed it, leaking this one tiny Weak
                    // allocation is safer than a callback use-after-free.
                    eprintln!(
                        "super-platinum: taskbar subclass removal failed; retaining weak callback slot"
                    );
                }
            }
            let state = self.state.get_mut();
            if state.receipt.label.is_some() {
                unsafe {
                    if let Ok(taskbar) = CoCreateInstance::<_, ITaskbarList3>(
                        &TaskbarList,
                        None,
                        CLSCTX_INPROC_SERVER,
                    ) {
                        let _ = taskbar.HrInit();
                        let _ =
                            taskbar.SetOverlayIcon(hwnd, HICON::default(), w!("No unread badge"));
                    }
                }
            }
        }
    }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // Main-thread weak slot ownership is retired exactly once; never unwind FFI.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let sink = unsafe { &*(data as *const Weak<Sink>) }.upgrade();
        if message == WM_NCDESTROY {
            let removed = unsafe { RemoveWindowSubclass(hwnd, Some(subclass), id) }.as_bool();
            if let Some(sink) = &sink {
                sink.hwnd.set(HWND::default());
                sink.hook_data.set(0);
            }
            if removed {
                unsafe {
                    drop(Box::from_raw(data as *mut Weak<Sink>));
                }
            }
            // On removal failure, keep the tiny weak slot alive through any
            // synchronous messages dispatched by DefSubclassProc during teardown.
        } else if let Some(sink) = sink {
            if message == sink.message {
                sink.pending_recreation.set(true);
                sink.flush_recreation();
            }
        }
    }));
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accessibility_preserves_full_count_and_symbol_meaning() {
        assert_eq!(description(Some("123456")), "123456 unread notifications");
        assert_eq!(description(Some("•")), "Unread messages");
        assert_eq!(description(None), "No unread badge");
    }
}
