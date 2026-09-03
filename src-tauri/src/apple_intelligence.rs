use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

// Define the response structure from Swift
#[repr(C)]
pub struct AppleLLMResponse {
    pub response: *mut c_char,
    pub success: c_int,
    pub error_message: *mut c_char,
}

// Link to the Swift functions
extern "C" {
    pub fn is_apple_intelligence_available() -> c_int;
    pub fn free_apple_llm_response(response: *mut AppleLLMResponse);
}

// Safe wrapper functions
pub fn check_apple_intelligence_availability() -> bool {
    unsafe { is_apple_intelligence_available() == 1 }
}

// Link to the Swift function for system prompt support
extern "C" {
    pub fn process_text_with_system_prompt_apple(
        system_prompt: *const c_char,
        user_content: *const c_char,
        max_tokens: i32,
    ) -> *mut AppleLLMResponse;
}

/// Process text with Apple Intelligence using separate system prompt and user content
pub fn process_text_with_system_prompt(
    system_prompt: &str,
    user_content: &str,
    max_tokens: i32,
) -> Result<String, String> {
    let system_cstr = CString::new(system_prompt).map_err(|e| e.to_string())?;
    let user_cstr = CString::new(user_content).map_err(|e| e.to_string())?;

    let response_ptr = unsafe {
        process_text_with_system_prompt_apple(system_cstr.as_ptr(), user_cstr.as_ptr(), max_tokens)
    };

    if response_ptr.is_null() {
        return Err("Null response from Apple LLM".to_string());
    }

    let response = unsafe { &*response_ptr };

    let result = if response.success == 1 {
        if response.response.is_null() {
            Ok(String::new())
        } else {
            let c_str = unsafe { CStr::from_ptr(response.response) };
            let rust_str = c_str.to_string_lossy().into_owned();
            Ok(rust_str)
        }
    } else {
        let error_c_str = if !response.error_message.is_null() {
            unsafe { CStr::from_ptr(response.error_message) }
        } else {
            c"Unknown error"
        };
        let error_msg = error_c_str.to_string_lossy().into_owned();
        Err(error_msg)
    };

    // Clean up the response
    unsafe { free_apple_llm_response(response_ptr) };

    result
}

// Link to the Swift function for correction-kind classification
extern "C" {
    pub fn check_vocabulary_apple(
        instructions: *const c_char,
        user_content: *const c_char,
    ) -> *mut AppleLLMResponse;
}

/// One classified correction pair as returned by Apple Intelligence.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct KindVerdict {
    /// The corrected text, copied from the pair the model was given.
    pub meant: String,
    /// Raw value of the Swift `CorrectionKind` enum, e.g. `personName`,
    /// `productOrCompany`, `acronym`, `commonWord`, `rewording`.
    pub kind: String,
}

/// Classify "heard -> meant" correction pairs by kind with Apple Intelligence.
///
/// `instructions` is the session's system instructions (the kind definitions);
/// `user_content` lists the numbered pairs. Guided generation on the Swift side
/// yields one verdict per pair in the order given, serialised as a JSON array of
/// `{"meant": ..., "kind": ...}` objects, which is decoded here. Any failure —
/// model unavailable, generation error, malformed JSON — is returned as `Err`.
pub fn check_vocabulary(
    instructions: &str,
    user_content: &str,
) -> Result<Vec<KindVerdict>, String> {
    let instructions_cstr = CString::new(instructions).map_err(|e| e.to_string())?;
    let user_cstr = CString::new(user_content).map_err(|e| e.to_string())?;

    let response_ptr =
        unsafe { check_vocabulary_apple(instructions_cstr.as_ptr(), user_cstr.as_ptr()) };

    if response_ptr.is_null() {
        return Err("Null response from Apple LLM".to_string());
    }

    let response = unsafe { &*response_ptr };

    let result = if response.success == 1 {
        if response.response.is_null() {
            Ok(String::new())
        } else {
            let c_str = unsafe { CStr::from_ptr(response.response) };
            Ok(c_str.to_string_lossy().into_owned())
        }
    } else {
        let error_c_str = if !response.error_message.is_null() {
            unsafe { CStr::from_ptr(response.error_message) }
        } else {
            c"Unknown error"
        };
        Err(error_c_str.to_string_lossy().into_owned())
    };

    // Clean up the response
    unsafe { free_apple_llm_response(response_ptr) };

    let json = result?;
    serde_json::from_str::<Vec<KindVerdict>>(&json)
        .map_err(|e| format!("Apple LLM returned malformed verdict JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_availability() {
        let available = check_apple_intelligence_availability();
        println!("Apple Intelligence available: {}", available);
    }
}
