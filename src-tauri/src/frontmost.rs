//! Identifies the application the user is dictating into.
//!
//! Sampled when a recording stops, which is the last moment before the
//! transcription pipeline runs and the user could switch windows. The result
//! is stored on the history entry so the insights page can attribute each
//! dictation to an app and, through the focused window's title, to what was
//! open inside a browser or terminal.
//!
//! `None` means the platform gave no usable answer (no accessibility support
//! on Linux, the API refused, no frontmost app).

/// The application that owned keyboard focus when a recording stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontmostApp {
    /// Stable identifier: the bundle identifier on macOS, the lowercased
    /// executable name (`slack.exe`) on Windows.
    pub app_id: Option<String>,
    /// Human-readable name shown in the insights page.
    pub name: Option<String>,
    /// Title of the focused window, when the platform exposes it.
    pub window_title: Option<String>,
}

pub fn frontmost_app() -> Option<FrontmostApp> {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_app()
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::frontmost_app()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2_app_kit::NSWorkspace;
    use objc2_core_foundation::{CFRetained, CFString, CFType};

    use super::FrontmostApp;
    use crate::focus::macos::{copy_attribute, AXUIElementRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32) -> i32;
    }

    /// The probe runs on the shortcut thread while the overlay is switching
    /// to its working state; an unresponsive app must not stall that.
    const MESSAGING_TIMEOUT_SECONDS: f32 = 0.5;

    pub fn frontmost_app() -> Option<FrontmostApp> {
        let workspace = NSWorkspace::sharedWorkspace();
        let running = workspace.frontmostApplication()?;
        let app_id = running.bundleIdentifier().map(|s| s.to_string());
        let name = running.localizedName().map(|s| s.to_string());
        let window_title = focused_window_title(running.processIdentifier());
        Some(FrontmostApp {
            app_id,
            name,
            window_title,
        })
    }

    /// Title of the frontmost app's focused window via the accessibility
    /// layer. Requires the accessibility permission Handy already needs for
    /// pasting; without it the attribute copy fails and this returns `None`.
    fn focused_window_title(pid: i32) -> Option<String> {
        // SAFETY: plain constructor; the result is a +1 CFType released by CFRetained.
        let app = unsafe { AXUIElementCreateApplication(pid) };
        let app: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new(app.cast::<CFType>())?) };
        let app_ref: AXUIElementRef = CFRetained::as_ptr(&app).as_ptr().cast::<c_void>();
        unsafe {
            AXUIElementSetMessagingTimeout(app_ref, MESSAGING_TIMEOUT_SECONDS);
        }
        let window = copy_attribute(app_ref, "AXFocusedWindow").ok()?;
        let window_ref: AXUIElementRef = CFRetained::as_ptr(&window).as_ptr().cast::<c_void>();
        let title = copy_attribute(window_ref, "AXTitle").ok()?;
        let title = title.downcast_ref::<CFString>()?.to_string();
        if title.trim().is_empty() {
            None
        } else {
            Some(title)
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    use super::FrontmostApp;

    pub fn frontmost_app() -> Option<FrontmostApp> {
        // SAFETY: documented Win32 entry points called with valid buffers; the
        // process handle is closed on every path after it is opened.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }

            let mut title_buf = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            let window_title = (title_len > 0)
                .then(|| String::from_utf16_lossy(&title_buf[..title_len as usize]))
                .filter(|t| !t.trim().is_empty());

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return Some(FrontmostApp {
                    app_id: None,
                    name: None,
                    window_title,
                });
            }

            let exe_path = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .ok()
                .and_then(|handle| {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    let ok = QueryFullProcessImageNameW(
                        handle,
                        PROCESS_NAME_WIN32,
                        PWSTR(buf.as_mut_ptr()),
                        &mut len,
                    )
                    .is_ok();
                    let _ = CloseHandle(handle);
                    ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
                });

            let exe_name = exe_path.as_deref().and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            });
            let name = exe_name.as_deref().map(|n| {
                std::path::Path::new(n)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| n.to_string())
            });

            Some(FrontmostApp {
                app_id: exe_name.map(|n| n.to_lowercase()),
                name,
                window_title,
            })
        }
    }
}
