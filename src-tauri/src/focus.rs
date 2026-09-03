//! Asks the operating system's accessibility layer whether the element that
//! currently has keyboard focus accepts typed text.
//!
//! Used right before a transcript is pasted: when nothing editable is focused
//! the paste chord lands nowhere, so the coordinator offers the transcript on
//! the overlay instead (see `copy_prompt`).
//!
//! The answer is tri-state. `None` means the platform gave no usable answer
//! (no accessibility support on Linux, the API refused, no frontmost app) and
//! callers must not draw conclusions from it.
//!
//! On macOS the focused field can also be captured as a [`FocusedTextField`]
//! and read back later from any thread.

#[cfg(target_os = "macos")]
pub use macos::FocusedTextField;

/// `Some(true)` when the focused element takes text input, `Some(false)` when
/// it definitely does not (including "nothing is focused"), `None` when the
/// platform could not tell.
pub fn focused_element_is_text_input() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        macos::focused_element_is_text_input()
    }
    #[cfg(target_os = "windows")]
    {
        windows_impl::focused_element_is_text_input()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// macOS accessibility roles that always take typed text.
#[cfg(any(target_os = "macos", test))]
const MACOS_TEXT_ROLES: [&str; 4] = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// Pure macOS classification: a known text role, or any element whose
/// `AXValue` the accessibility layer lets us set (editable web content, code
/// editors and terminals that report a generic role but writable text).
#[cfg(any(target_os = "macos", test))]
fn classify_macos_role(role: &str, value_settable: bool) -> bool {
    MACOS_TEXT_ROLES.contains(&role) || value_settable
}

/// The role (and, in web content, subrole) of a password field.
#[cfg(any(target_os = "macos", test))]
const MACOS_SECURE_TEXT_ROLE: &str = "AXSecureTextField";

/// Pure macOS check: a field whose contents must never be read back. Native
/// password fields report the role; WebKit reports a generic text role with
/// the secure subrole.
#[cfg(any(target_os = "macos", test))]
fn macos_role_is_secure(role: &str, subrole: Option<&str>) -> bool {
    role == MACOS_SECURE_TEXT_ROLE || subrole == Some(MACOS_SECURE_TEXT_ROLE)
}

/// Converts a `CFRange` (signed, `kCFNotFound` = -1 for "no range") into a
/// `(location, length)` pair. Either bound negative means no selection.
#[cfg(any(target_os = "macos", test))]
fn utf16_range(location: isize, length: isize) -> Option<(usize, usize)> {
    Some((
        usize::try_from(location).ok()?,
        usize::try_from(length).ok()?,
    ))
}

/// UI Automation control type ids (`UIA_CONTROLTYPE_ID`). Listed as plain
/// integers so the classifier can be unit-tested on every platform.
#[cfg(any(target_os = "windows", test))]
mod uia {
    pub const COMBO_BOX: i32 = 50003;
    pub const EDIT: i32 = 50004;
    pub const DOCUMENT: i32 = 50030;
}

/// Pure Windows classification. `Edit` and `ComboBox` controls take text by
/// definition. `Document` is shared by editors and read-only surfaces such as
/// a browser page, so it and every other control type count only when the
/// Value pattern reports a writable value.
#[cfg(any(target_os = "windows", test))]
fn classify_windows_control(control_type: i32, value_writable: Option<bool>) -> bool {
    match control_type {
        uia::EDIT | uia::COMBO_BOX => true,
        uia::DOCUMENT => value_writable == Some(true),
        _ => value_writable == Some(true),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2_app_kit::NSRunningApplication;
    use objc2_core_foundation::{CFEqual, CFGetTypeID, CFRange, CFRetained, CFString, CFType};

    type AXUIElementRef = *mut c_void;
    type AXValueRef = *mut c_void;
    type AXError = i32;
    type AXValueType = u32;
    type CFTypeID = usize;

    const AX_ERROR_SUCCESS: AXError = 0;
    /// The attribute exists but has no value: no element has focus.
    const AX_ERROR_NO_VALUE: AXError = -25212;
    /// `kAXValueTypeCFRange`: the AXValue wraps a `CFRange`.
    const AX_VALUE_TYPE_CFRANGE: AXValueType = 4;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: &CFString,
            value: *mut *const CFType,
        ) -> AXError;
        fn AXUIElementIsAttributeSettable(
            element: AXUIElementRef,
            attribute: &CFString,
            settable: *mut u8,
        ) -> AXError;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32)
            -> AXError;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
        #[cfg(test)]
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXValueGetTypeID() -> CFTypeID;
        fn AXValueGetValue(
            value: AXValueRef,
            value_type: AXValueType,
            value_ptr: *mut c_void,
        ) -> u8;
    }

    /// Upper bound on how long a single accessibility request may block. The
    /// query runs on the main thread right before the paste, so an unresponsive
    /// frontmost app must not freeze the UI for the default six seconds.
    const MESSAGING_TIMEOUT_SECONDS: f32 = 0.5;

    /// Raw `AXUIElementRef` view of a retained element, for passing to the
    /// accessibility C API. Valid for as long as the `CFRetained` lives.
    fn element_ref(element: &CFRetained<CFType>) -> AXUIElementRef {
        CFRetained::as_ptr(element).as_ptr().cast()
    }

    /// The system-wide accessibility element with the messaging timeout
    /// applied. The timeout is inherited by every element derived from it
    /// (the focused element included), so one call bounds a whole query.
    fn system_wide_element() -> Option<CFRetained<CFType>> {
        // SAFETY: plain constructor; the result is a +1 CFType that CFRetained
        // releases when dropped.
        let system_wide = unsafe { AXUIElementCreateSystemWide() };
        let system_wide: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new(system_wide.cast::<CFType>())?) };
        // SAFETY: `system_wide` is a live AXUIElement for the duration of the call.
        unsafe {
            AXUIElementSetMessagingTimeout(element_ref(&system_wide), MESSAGING_TIMEOUT_SECONDS);
        }
        Some(system_wide)
    }

    /// Copies an attribute value (Copy rule: the caller owns the result).
    fn copy_attribute(element: AXUIElementRef, name: &str) -> Result<CFRetained<CFType>, AXError> {
        let attribute = CFString::from_str(name);
        let mut value: *const CFType = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(element, &attribute, &mut value) };
        if err != AX_ERROR_SUCCESS {
            return Err(err);
        }
        let ptr = NonNull::new(value.cast_mut()).ok_or(AX_ERROR_NO_VALUE)?;
        // SAFETY: AXUIElementCopyAttributeValue follows the Create/Copy rule,
        // so the pointer carries a +1 retain that CFRetained now owns.
        Ok(unsafe { CFRetained::from_raw(ptr) })
    }

    /// Copies a string attribute. A value that is not a `CFString` reads as
    /// the empty string; a failed request is the error.
    fn copy_string_attribute(element: AXUIElementRef, name: &str) -> Result<String, AXError> {
        copy_attribute(element, name).map(|value| {
            value
                .downcast_ref::<CFString>()
                .map(|s| s.to_string())
                .unwrap_or_default()
        })
    }

    fn attribute_is_settable(element: AXUIElementRef, name: &str) -> bool {
        let attribute = CFString::from_str(name);
        let mut settable: u8 = 0;
        let err = unsafe { AXUIElementIsAttributeSettable(element, &attribute, &mut settable) };
        err == AX_ERROR_SUCCESS && settable != 0
    }

    pub fn focused_element_is_text_input() -> Option<bool> {
        let system_wide = system_wide_element()?;

        let focused = match copy_attribute(element_ref(&system_wide), "AXFocusedUIElement") {
            Ok(element) => element,
            Err(AX_ERROR_NO_VALUE) => return Some(false),
            Err(err) => {
                log::debug!("focus check: AXFocusedUIElement unavailable (AXError {err})");
                return None;
            }
        };
        let focused_ref = element_ref(&focused);

        let role = match copy_string_attribute(focused_ref, "AXRole") {
            Ok(role) => role,
            Err(err) => {
                log::debug!("focus check: AXRole unavailable (AXError {err})");
                return None;
            }
        };
        let value_settable = attribute_is_settable(focused_ref, "AXValue");
        let is_text = super::classify_macos_role(&role, value_settable);
        log::debug!(
            "focus check: role={role} value_settable={value_settable} text_input={is_text}"
        );
        Some(is_text)
    }

    /// A text field that had keyboard focus when captured. Holds a retained
    /// `AXUIElementRef`; AXUIElement messaging is thread-safe, so the handle
    /// may be read from any thread.
    pub struct FocusedTextField {
        element: CFRetained<CFType>,
        /// Process id of the owning application, read once at capture.
        pid: Option<i32>,
    }

    // SAFETY: the only state is an immutable retained AXUIElement, which the
    // accessibility API allows to be messaged from any thread, plus a plain
    // integer.
    unsafe impl Send for FocusedTextField {}
    // SAFETY: as for `Send`; every method takes `&self` and only issues
    // read-only accessibility requests.
    unsafe impl Sync for FocusedTextField {}

    impl FocusedTextField {
        /// The element with keyboard focus, if it is an editable text field
        /// that may be read back. `None` when nothing is focused, when the
        /// element is not a text input, or when it is a secure (password)
        /// field.
        pub fn capture() -> Option<Self> {
            let system_wide = system_wide_element()?;
            Self::from_focused_element_of(&system_wide)
        }

        /// The focused element of one application, resolved through that
        /// application's own accessibility element instead of the system-wide
        /// one. The system-wide lookup fails with `kAXErrorCannotComplete`
        /// from a plain shell process, so the ignored test reads a known
        /// application this way.
        #[cfg(test)]
        pub(super) fn capture_in_application(pid: i32) -> Option<Self> {
            // SAFETY: plain constructor; the result is a +1 CFType that
            // CFRetained releases when dropped.
            let app = unsafe { AXUIElementCreateApplication(pid) };
            let app: CFRetained<CFType> =
                unsafe { CFRetained::from_raw(NonNull::new(app.cast::<CFType>())?) };
            Self::from_focused_element_of(&app)
        }

        /// Reads `AXFocusedUIElement` of `parent` and wraps it if it is a
        /// readable text field.
        fn from_focused_element_of(parent: &CFRetained<CFType>) -> Option<Self> {
            let element = match copy_attribute(element_ref(parent), "AXFocusedUIElement") {
                Ok(element) => element,
                Err(err) => {
                    log::debug!("focus capture: AXFocusedUIElement unavailable (AXError {err})");
                    return None;
                }
            };
            let raw = element_ref(&element);
            // The handle outlives `parent` and is messaged again later, so it
            // carries its own timeout.
            // SAFETY: `element` is a live AXUIElement for the duration of the call.
            unsafe {
                AXUIElementSetMessagingTimeout(raw, MESSAGING_TIMEOUT_SECONDS);
            }

            let role = match copy_string_attribute(raw, "AXRole") {
                Ok(role) => role,
                Err(err) => {
                    log::debug!("focus capture: AXRole unavailable (AXError {err})");
                    return None;
                }
            };
            let subrole = copy_string_attribute(raw, "AXSubrole").ok();
            if super::macos_role_is_secure(&role, subrole.as_deref()) {
                log::debug!("focus capture: secure field, not captured");
                return None;
            }
            let value_settable = attribute_is_settable(raw, "AXValue");
            if !super::classify_macos_role(&role, value_settable) {
                log::debug!("focus capture: role={role} is not a text input");
                return None;
            }

            let mut pid: i32 = 0;
            // SAFETY: `raw` is a live AXUIElement and `pid` is a valid out-pointer.
            let err = unsafe { AXUIElementGetPid(raw, &mut pid) };
            let pid = (err == AX_ERROR_SUCCESS && pid > 0).then_some(pid);

            log::debug!("focus capture: role={role} subrole={subrole:?} pid={pid:?}");
            Some(Self { element, pid })
        }

        /// The field's current text (`AXValue` as a `CFString`). `None` if the
        /// element is gone, the attribute cannot be read, or the value is not
        /// a string; never a truncated string.
        pub fn value(&self) -> Option<String> {
            let value = copy_attribute(element_ref(&self.element), "AXValue").ok()?;
            Some(value.downcast_ref::<CFString>()?.to_string())
        }

        /// The field's placeholder (`AXPlaceholderValue`), shown when it is
        /// empty. Some toolkits report it as the value once the text is gone.
        pub fn placeholder(&self) -> Option<String> {
            copy_string_attribute(element_ref(&self.element), "AXPlaceholderValue")
                .ok()
                .filter(|s| !s.trim().is_empty())
        }

        /// The selection as `(location, length)` in UTF-16 code units, from
        /// `AXSelectedTextRange`. `None` if unavailable.
        pub fn selection_utf16(&self) -> Option<(usize, usize)> {
            let value = copy_attribute(element_ref(&self.element), "AXSelectedTextRange").ok()?;
            // SAFETY: AXValueGetTypeID has no preconditions; CFGetTypeID takes a
            // live CF object.
            let is_ax_value = unsafe { AXValueGetTypeID() } == CFGetTypeID(Some(&value));
            if !is_ax_value {
                return None;
            }
            let mut range = CFRange {
                location: 0,
                length: 0,
            };
            // SAFETY: `value` is an AXValue (checked above) and `range` is a
            // correctly sized, writable CFRange; AXValueGetValue writes it only
            // when the AXValue's type matches the requested kAXValueTypeCFRange
            // and reports false otherwise.
            let ok = unsafe {
                AXValueGetValue(
                    CFRetained::as_ptr(&value).as_ptr().cast(),
                    AX_VALUE_TYPE_CFRANGE,
                    (&mut range as *mut CFRange).cast(),
                )
            };
            if ok == 0 {
                return None;
            }
            super::utf16_range(range.location, range.length)
        }

        /// True while this element is still the system-wide focused element.
        pub fn is_focused(&self) -> bool {
            let Some(system_wide) = system_wide_element() else {
                return false;
            };
            match copy_attribute(element_ref(&system_wide), "AXFocusedUIElement") {
                Ok(focused) => CFEqual(Some(&focused), Some(&self.element)),
                Err(_) => false,
            }
        }

        /// Process id of the owning application.
        pub fn pid(&self) -> Option<i32> {
            self.pid
        }

        /// Bundle identifier of the owning application. `None` if the process
        /// is unknown, has exited, or is not an application bundle.
        pub fn bundle_id(&self) -> Option<String> {
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(self.pid?)?;
            app.bundleIdentifier().map(|id| id.to_string())
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ValuePatternId,
    };

    pub fn focused_element_is_text_input() -> Option<bool> {
        // SAFETY: every call below is a documented COM/UIA entry point used with
        // valid arguments; interface pointers are owned by windows-rs wrappers.
        unsafe {
            // The calling thread may already hold an apartment (S_FALSE) or one of
            // a different model (RPC_E_CHANGED_MODE); both leave COM usable. Only a
            // successful init has to be balanced by CoUninitialize.
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let result = query_focus();
            if init.is_ok() {
                CoUninitialize();
            }
            result
        }
    }

    unsafe fn query_focus() -> Option<bool> {
        let automation: IUIAutomation =
            match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(automation) => automation,
                Err(err) => {
                    log::debug!("focus check: UI Automation unavailable ({err})");
                    return None;
                }
            };
        let element = match automation.GetFocusedElement() {
            Ok(element) => element,
            Err(err) => {
                log::debug!("focus check: no focused element ({err})");
                return None;
            }
        };
        let control_type = element.CurrentControlType().ok()?;
        let value_writable = element
            .GetCurrentPattern(UIA_ValuePatternId)
            .ok()
            .and_then(|pattern| pattern.cast::<IUIAutomationValuePattern>().ok())
            .and_then(|value| value.CurrentIsReadOnly().ok())
            .map(|read_only| !read_only.as_bool());
        let is_text = super::classify_windows_control(control_type.0, value_writable);
        log::debug!(
            "focus check: control_type={} value_writable={value_writable:?} text_input={is_text}",
            control_type.0
        );
        Some(is_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_text_roles_are_text_inputs() {
        for role in MACOS_TEXT_ROLES {
            assert!(classify_macos_role(role, false), "{role}");
        }
    }

    #[test]
    fn macos_generic_role_with_settable_value_is_text_input() {
        assert!(classify_macos_role("AXWebArea", true));
    }

    #[test]
    fn macos_static_content_is_not_text_input() {
        assert!(!classify_macos_role("AXStaticText", false));
        assert!(!classify_macos_role("AXButton", false));
        assert!(!classify_macos_role("", false));
    }

    #[test]
    fn macos_secure_role_or_subrole_is_secure() {
        assert!(macos_role_is_secure("AXSecureTextField", None));
        assert!(macos_role_is_secure(
            "AXTextField",
            Some("AXSecureTextField")
        ));
        assert!(macos_role_is_secure(
            "AXSecureTextField",
            Some("AXSecureTextField")
        ));
    }

    #[test]
    fn macos_plain_text_field_is_not_secure() {
        assert!(!macos_role_is_secure("AXTextField", None));
        assert!(!macos_role_is_secure("AXTextField", Some("AXSearchField")));
        assert!(!macos_role_is_secure("AXTextArea", Some("")));
        assert!(!macos_role_is_secure("", None));
    }

    #[test]
    fn utf16_range_accepts_non_negative_bounds() {
        assert_eq!(utf16_range(0, 0), Some((0, 0)));
        assert_eq!(utf16_range(7, 3), Some((7, 3)));
    }

    #[test]
    fn utf16_range_rejects_negative_bounds() {
        assert_eq!(utf16_range(-1, 0), None);
        assert_eq!(utf16_range(0, -1), None);
        assert_eq!(utf16_range(-1, -1), None);
    }

    /// Forwards `log::debug!` lines to stderr so the ignored capture test
    /// shows which accessibility request failed.
    #[cfg(target_os = "macos")]
    struct StderrLogger;

    #[cfg(target_os = "macos")]
    impl log::Log for StderrLogger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            eprintln!("{}: {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }

    /// Reads whatever text field is focused on this Mac. Needs accessibility
    /// permission for the test binary's host process; run with
    /// `cargo test --lib focus -- --ignored --nocapture`. The system-wide
    /// focus lookup fails with `kAXErrorCannotComplete` from a plain shell,
    /// so `HANDY_FOCUS_TEST_PID=<pid>` reads the focused field of that
    /// application instead.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn macos_capture_prints_focused_field() {
        let _ = log::set_logger(&StderrLogger);
        log::set_max_level(log::LevelFilter::Debug);
        let field = match std::env::var("HANDY_FOCUS_TEST_PID") {
            Ok(pid) => {
                let pid = pid
                    .parse()
                    .expect("HANDY_FOCUS_TEST_PID must be an integer");
                FocusedTextField::capture_in_application(pid)
            }
            Err(_) => FocusedTextField::capture(),
        };
        match field {
            Some(field) => {
                println!("pid: {:?}", field.pid());
                println!("bundle_id: {:?}", field.bundle_id());
                println!("is_focused: {}", field.is_focused());
                println!("selection_utf16: {:?}", field.selection_utf16());
                println!("value: {:?}", field.value());
            }
            None => println!("no readable text field is focused"),
        }
    }

    #[test]
    fn windows_edit_and_combo_box_are_text_inputs_regardless_of_value_pattern() {
        assert!(classify_windows_control(uia::EDIT, None));
        assert!(classify_windows_control(uia::EDIT, Some(false)));
        assert!(classify_windows_control(uia::COMBO_BOX, None));
    }

    #[test]
    fn windows_document_needs_writable_value() {
        assert!(classify_windows_control(uia::DOCUMENT, Some(true)));
        assert!(!classify_windows_control(uia::DOCUMENT, Some(false)));
        assert!(!classify_windows_control(uia::DOCUMENT, None));
    }

    #[test]
    fn windows_other_controls_need_writable_value() {
        const BUTTON: i32 = 50000;
        assert!(!classify_windows_control(BUTTON, None));
        assert!(!classify_windows_control(BUTTON, Some(false)));
        assert!(classify_windows_control(BUTTON, Some(true)));
    }
}
