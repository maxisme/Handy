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

    use objc2_core_foundation::{CFRetained, CFString, CFType};

    type AXUIElementRef = *mut c_void;
    type AXError = i32;

    const AX_ERROR_SUCCESS: AXError = 0;
    /// The attribute exists but has no value: no element has focus.
    const AX_ERROR_NO_VALUE: AXError = -25212;

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
    }

    /// Upper bound on how long a single accessibility request may block. The
    /// query runs on the main thread right before the paste, so an unresponsive
    /// frontmost app must not freeze the UI for the default six seconds.
    const MESSAGING_TIMEOUT_SECONDS: f32 = 0.5;

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

    fn attribute_is_settable(element: AXUIElementRef, name: &str) -> bool {
        let attribute = CFString::from_str(name);
        let mut settable: u8 = 0;
        let err = unsafe { AXUIElementIsAttributeSettable(element, &attribute, &mut settable) };
        err == AX_ERROR_SUCCESS && settable != 0
    }

    pub fn focused_element_is_text_input() -> Option<bool> {
        // SAFETY: plain constructor; the result is a +1 CFType we release below.
        let system_wide = unsafe { AXUIElementCreateSystemWide() };
        let system_wide: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new(system_wide.cast::<CFType>())?) };
        // Applies to every element derived from this one (the focused element
        // inherits it), so one call bounds the whole query.
        unsafe {
            AXUIElementSetMessagingTimeout(
                CFRetained::as_ptr(&system_wide).as_ptr().cast(),
                MESSAGING_TIMEOUT_SECONDS,
            );
        }

        let focused = match copy_attribute(
            CFRetained::as_ptr(&system_wide).as_ptr().cast(),
            "AXFocusedUIElement",
        ) {
            Ok(element) => element,
            Err(AX_ERROR_NO_VALUE) => return Some(false),
            Err(err) => {
                log::debug!("focus check: AXFocusedUIElement unavailable (AXError {err})");
                return None;
            }
        };
        let focused_ref: AXUIElementRef = CFRetained::as_ptr(&focused).as_ptr().cast();

        let role = match copy_attribute(focused_ref, "AXRole") {
            Ok(value) => value
                .downcast_ref::<CFString>()
                .map(|s| s.to_string())
                .unwrap_or_default(),
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
