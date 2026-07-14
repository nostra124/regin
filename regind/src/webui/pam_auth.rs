//! Minimal, hand-written PAM FFI bindings (FEAT-087, acceptance criterion
//! 5) — deliberately **not** the `pam`/`pam-sys` crates, which pull in
//! `bindgen`/`libclang` as a build dependency (a much heavier requirement
//! than the ticket's own "libpam development headers" build note implies).
//! PAM's C API for a single authenticate-and-check-account challenge is
//! small and ABI-stable, so it's hand-declared here: `pam_start`, a
//! conversation callback that answers `PAM_PROMPT_ECHO_OFF`/`_ON` prompts
//! with the supplied password, `pam_authenticate`, `pam_acct_mgmt`,
//! `pam_end`. Links against `libpam.so` via `build.rs`.

use anyhow::{Result, bail};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 6;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type PamConvFn =
    unsafe extern "C" fn(num_msg: c_int, msg: *mut *const PamMessage, resp: *mut *mut PamResponse, appdata_ptr: *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: PamConvFn,
    appdata_ptr: *mut c_void,
}

#[allow(non_camel_case_types)]
enum pam_handle_t {}

unsafe extern "C" {
    fn pam_start(service_name: *const c_char, user: *const c_char, pam_conversation: *const PamConv, pamh: *mut *mut pam_handle_t) -> c_int;
    fn pam_authenticate(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut pam_handle_t, pam_status: c_int) -> c_int;
}

/// Answers every `PAM_PROMPT_ECHO_OFF`/`_ON` message with the password
/// passed via `appdata_ptr` (a `*const CString`, set up by [`authenticate`]).
/// Response strings are allocated with the C allocator (`libc::malloc`) —
/// per the PAM conversation contract, libpam takes ownership and frees
/// them; allocating with Rust's global allocator instead would be a
/// mismatched-allocator bug.
unsafe extern "C" fn conversation(num_msg: c_int, msg: *mut *const PamMessage, resp: *mut *mut PamResponse, appdata_ptr: *mut c_void) -> c_int {
    if num_msg <= 0 || appdata_ptr.is_null() || msg.is_null() {
        return PAM_CONV_ERR;
    }
    let n = num_msg as usize;
    let password = unsafe { &*(appdata_ptr as *const CString) };

    let resp_array = unsafe { libc::calloc(n, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if resp_array.is_null() {
        return PAM_BUF_ERR;
    }

    for i in 0..n {
        let message = unsafe { *msg.add(i) };
        if message.is_null() {
            continue;
        }
        let style = unsafe { (*message).msg_style };
        let entry = unsafe { &mut *resp_array.add(i) };
        entry.resp_retcode = 0;
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            let bytes = password.as_bytes_with_nul();
            let buf = unsafe { libc::malloc(bytes.len()) } as *mut c_char;
            if buf.is_null() {
                unsafe { libc::free(resp_array as *mut c_void) };
                return PAM_BUF_ERR;
            }
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, bytes.len()) };
            entry.resp = buf;
        }
    }

    unsafe { *resp = resp_array };
    PAM_SUCCESS
}

/// Authenticate `username`/`password` against PAM service `service`
/// (`pam_authenticate` + `pam_acct_mgmt`). `Ok(true)`/`Ok(false)` for a
/// clean accept/reject; `Err` only for a PAM-level setup failure
/// (`pam_start` itself failing — a missing/misconfigured service file,
/// not a wrong password).
pub fn authenticate(service: &str, username: &str, password: &str) -> Result<bool> {
    let service_c = CString::new(service)?;
    let user_c = CString::new(username)?;
    let password_c = CString::new(password)?;

    let conv = PamConv { conv: conversation, appdata_ptr: &password_c as *const CString as *mut c_void };

    let mut handle: *mut pam_handle_t = ptr::null_mut();
    let start_rc = unsafe { pam_start(service_c.as_ptr(), user_c.as_ptr(), &conv, &mut handle) };
    if start_rc != PAM_SUCCESS || handle.is_null() {
        bail!("pam_start failed for service {service:?} (code {start_rc})");
    }

    let auth_rc = unsafe { pam_authenticate(handle, 0) };
    let success = if auth_rc == PAM_SUCCESS {
        unsafe { pam_acct_mgmt(handle, 0) == PAM_SUCCESS }
    } else {
        false
    };

    unsafe { pam_end(handle, auth_rc) };
    Ok(success)
}

/// `CStr::to_string_lossy` shorthand used by call sites that need to log a
/// PAM service/user name; kept here so no other module needs `CStr` at all.
#[allow(dead_code)]
pub fn describe(c: &CStr) -> String {
    c.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the *real* libpam + the real conversation callback,
    // against two throwaway PAM service files this ticket's test setup
    // creates at `/etc/pam.d/regin-test-permit` (`pam_permit.so` — always
    // succeeds) and `/etc/pam.d/regin-test-deny` (`pam_deny.so` — always
    // fails). Both modules ship with every standard libpam install and are
    // the conventional way to test a PAM integration without needing real
    // user credentials or root-owned shadow access.

    #[test]
    fn pam_permit_service_always_succeeds() {
        if !std::path::Path::new("/etc/pam.d/regin-test-permit").exists() {
            eprintln!("skipping: /etc/pam.d/regin-test-permit not present in this environment");
            return;
        }
        let ok = authenticate("regin-test-permit", "anyuser", "anypassword").unwrap();
        assert!(ok);
    }

    #[test]
    fn pam_deny_service_always_fails() {
        if !std::path::Path::new("/etc/pam.d/regin-test-deny").exists() {
            eprintln!("skipping: /etc/pam.d/regin-test-deny not present in this environment");
            return;
        }
        let ok = authenticate("regin-test-deny", "anyuser", "anypassword").unwrap();
        assert!(!ok);
    }

    #[test]
    fn an_unconfigured_service_fails_to_even_start() {
        let result = authenticate("regin-test-service-that-does-not-exist", "u", "p");
        assert!(result.is_err() || !result.unwrap());
    }
}
