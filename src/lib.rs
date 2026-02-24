#![allow(clippy::not_unsafe_ptr_arg_deref)]

#[cfg(feature = "python")]
use pyo3::exceptions::{PyRuntimeError, PyValueError};
#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyDict;
use std::ffi::{CStr, CString};
#[cfg(feature = "python")]
use std::fs;
#[cfg(feature = "python")]
use std::io;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
#[cfg(feature = "python")]
use std::path::Path;
#[cfg(feature = "python")]
use std::path::PathBuf;

pub mod crypto;
pub mod polish;
pub mod rotator;
pub mod tunnel;
pub mod types;
pub mod verifier;

#[cfg(feature = "python")]
use types::Proxy;

// Helper to load files
#[cfg(feature = "python")]
fn load_json_array(path: &Path) -> io::Result<Vec<Proxy>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let proxies: Vec<Proxy> = serde_json::from_str(&raw).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), e),
        )
    })?;
    Ok(proxies)
}

#[cfg(feature = "python")]
fn load_all_pools(workspace: &Path) -> io::Result<(Vec<Proxy>, Vec<Proxy>, Vec<Proxy>)> {
    let dns = load_json_array(&workspace.join("proxies_dns.json"))?;
    let non_dns = load_json_array(&workspace.join("proxies_non_dns.json"))?;
    let combined = load_json_array(&workspace.join("proxies_combined.json"))?;
    Ok((dns, non_dns, combined))
}

#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (mode, workspace=None))]
fn build_chain(py: Python<'_>, mode: &str, workspace: Option<&str>) -> PyResult<PyObject> {
    let mode = mode.to_lowercase();
    let ws = workspace
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (dns, non_dns, combined) = load_all_pools(&ws).map_err(|e| {
        PyRuntimeError::new_err(format!(
            "Failed to load pools from '{}': {}",
            ws.display(),
            e
        ))
    })?;

    let decision =
        rotator::build_chain_decision(&mode, &dns, &non_dns, &combined).ok_or_else(|| {
            PyRuntimeError::new_err(format!("Failed to build chain for mode='{}'", mode))
        })?;

    // Convert RotationDecision -> Python dict
    let result = PyDict::new(py);
    result.set_item("mode", decision.mode)?;
    result.set_item("timestamp", decision.timestamp)?;
    result.set_item("chain_id", decision.chain_id)?;
    result.set_item("avg_latency", decision.avg_latency)?;
    result.set_item("min_score", decision.min_score)?;
    result.set_item("max_score", decision.max_score)?;

    // Chain hops
    let hops = decision
        .chain
        .iter()
        .enumerate()
        .map(|(i, hop)| {
            let d = PyDict::new(py);
            d.set_item("index", i + 1)?;
            d.set_item("ip", &hop.ip)?;
            d.set_item("port", hop.port)?;
            d.set_item("proto", &hop.proto)?;
            d.set_item("country", &hop.country)?;
            d.set_item("latency", hop.latency)?;
            d.set_item("score", hop.score)?;
            Ok(d.into())
        })
        .collect::<PyResult<Vec<PyObject>>>()?;
    result.set_item("chain", hops)?;

    // Encryption
    let enc = decision
        .encryption
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let d = PyDict::new(py);
            d.set_item("hop", i + 1)?;
            d.set_item("key_hex", &ch.key_hex)?;
            d.set_item("nonce_hex", &ch.nonce_hex)?;
            Ok(d.into())
        })
        .collect::<PyResult<Vec<PyObject>>>()?;
    result.set_item("encryption", enc)?;

    Ok(result.into())
}

#[cfg(feature = "python")]
#[pyfunction]
fn validate_mode(mode: &str) -> PyResult<()> {
    let m = mode.to_lowercase();
    let allowed = ["lite", "stealth", "high", "phantom"];
    if allowed.contains(&m.as_str()) {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "Invalid mode '{}'. Allowed: lite, stealth, high, phantom",
            mode
        )))
    }
}

#[cfg(feature = "python")]
#[pyfunction]
fn version() -> PyResult<String> {
    Ok("rotator_rs_pyo3_v1".to_string())
}

#[cfg(feature = "python")]
#[pymodule]
fn rotator_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_chain, m)?)?;
    m.add_function(wrap_pyfunction!(validate_mode, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

// ==========================================
// C API for Go Integration
// ==========================================

/// Validates that a mode string is one of the allowed values
fn validate_mode_string(mode: &str) -> bool {
    let normalized = mode.to_lowercase();
    matches!(normalized.as_str(), "lite" | "stealth" | "high" | "phantom")
}

/// Validates JSON array structure before processing
/// Returns true if valid, false otherwise
fn validate_json_array(json_str: &str) -> bool {
    // Check for empty input
    if json_str.trim().is_empty() {
        log::debug!("Empty JSON input");
        return false;
    }

    // Parse to verify it's a valid JSON array
    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(value) => value.is_array(),
        Err(e) => {
            log::debug!("Invalid JSON: {}", e);
            false
        }
    }
}

/// Helper function to initialize the logger.
/// Uses try_init to avoid panicking if logger is already initialized.
fn init_logger() {
    let _ = env_logger::try_init();
}

/// Helper to safely execute FFI code with panic catching and logging.
/// Returns None on panic, which the caller converts to null pointer.
/// Used for FFI functions that return Option<T>.
fn catch_unwind_ffi<F, R>(func: F, operation: &str) -> Option<R>
where
    F: FnOnce() -> Option<R> + panic::UnwindSafe,
{
    match panic::catch_unwind(AssertUnwindSafe(func)) {
        Ok(result) => result,
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            log::error!(
                "Panic caught in FFI operation '{}': {}",
                operation,
                panic_msg
            );
            None
        }
    }
}

/// Helper to safely execute FFI code with panic catching and logging for void functions.
/// Used for FFI functions that return ().
fn catch_unwind_ffi_void<F>(func: F, operation: &str)
where
    F: FnOnce() + panic::UnwindSafe,
{
    match panic::catch_unwind(AssertUnwindSafe(func)) {
        Ok(_) => {}
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            log::error!(
                "Panic caught in FFI operation '{}': {}",
                operation,
                panic_msg
            );
        }
    }
}

#[no_mangle]
pub extern "C" fn run_polish_c(raw_json: *const c_char) -> *mut c_char {
    // Initialize logger (safe to call multiple times due to try_init)
    init_logger();

    // Wrap entire function in panic catch to prevent panics crossing FFI boundary
    let result = catch_unwind_ffi(
        || {
            // Validate pointer is not null
            if raw_json.is_null() {
                log::error!("run_polish_c called with null pointer");
                return None;
            }

            let c_str = unsafe { CStr::from_ptr(raw_json) };

            // Validate UTF-8 encoding
            let json_str = match c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("run_polish_c: Invalid UTF-8 in input: {}", e);
                    return None;
                }
            };

            // Validate JSON is not empty
            if json_str.trim().is_empty() {
                log::error!("run_polish_c: Empty JSON input");
                return None;
            }

            // Validate JSON structure before processing
            if !validate_json_array(json_str) {
                log::error!("run_polish_c: Invalid JSON array structure");
                return None;
            }

            // Parse the JSON into Proxy objects
            let proxies: Vec<types::Proxy> = match serde_json::from_str(json_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!(
                        "run_polish_c: Failed to parse proxy JSON: {} | Input preview: {:.100}",
                        e,
                        json_str
                    );
                    return None;
                }
            };

            // Validate proxy data before processing
            for (i, proxy) in proxies.iter().enumerate() {
                // Validate IP address format (basic check)
                if proxy.ip.is_empty() || proxy.ip.len() > 255 {
                    log::error!(
                        "run_polish_c: Invalid IP at index {}: length {}",
                        i,
                        proxy.ip.len()
                    );
                    return None;
                }

                // Validate port is non-zero
                if proxy.port == 0 {
                    log::error!(
                        "run_polish_c: Invalid port at index {}: port cannot be zero",
                        i
                    );
                    return None;
                }

                // Validate protocol is not empty
                if proxy.proto.is_empty() || proxy.proto.len() > 32 {
                    log::error!(
                        "run_polish_c: Invalid protocol at index {}: {}",
                        i,
                        proxy.proto
                    );
                    return None;
                }
            }

            let unique = polish::deduplicate_proxies(proxies);
            let scored = polish::calculate_scores(unique);
            let (dns, non_dns) = polish::split_proxy_pools(scored.clone());

            #[derive(serde::Serialize)]
            struct PolishResult {
                dns: Vec<types::Proxy>,
                non_dns: Vec<types::Proxy>,
                combined: Vec<types::Proxy>,
            }

            let result = PolishResult {
                dns,
                non_dns,
                combined: scored,
            };
            let out_json = match serde_json::to_string(&result) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("run_polish_c: Failed to serialize polish result: {}", e);
                    return None;
                }
            };

            match CString::new(out_json) {
                Ok(c_string) => Some(c_string.into_raw()),
                Err(e) => {
                    log::error!("run_polish_c: Failed to create C string from result: {}", e);
                    None
                }
            }
        },
        "run_polish_c",
    );

    // Return null pointer on panic or error
    result.unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn run_verify_c(proxies_json: *const c_char) -> *mut c_char {
    init_logger();
    let result = catch_unwind_ffi(
        || {
            if proxies_json.is_null() {
                log::error!("run_verify_c: proxies_json is null");
                return None;
            }

            let c_str = unsafe { CStr::from_ptr(proxies_json) };
            let json_str = match c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!("run_verify_c: Invalid UTF-8 in proxies_json: {}", e);
                    return None;
                }
            };

            let proxies: Vec<types::Proxy> = match serde_json::from_str(json_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("run_verify_c: Failed to parse proxies JSON: {}", e);
                    return None;
                }
            };

            // Run in a separate thread to isolate Tokio runtime from CGO's thread scheduler.
            // This is CRITICAL to prevent deadlocks when CGO threads park.
            let handle = std::thread::spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("run_verify_c: Failed to build tokio runtime: {}", e);
                        return None;
                    }
                };

                Some(rt.block_on(verifier::verify_pool(proxies)))
            });

            let verified = match handle.join() {
                Ok(Some(v)) => v,
                _ => {
                    log::error!("run_verify_c: Tokio thread panicked or runtime failed");
                    return None;
                }
            };

            let out_json = match serde_json::to_string(&verified) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("run_verify_c: Failed to serialize verified proxies: {}", e);
                    return None;
                }
            };

            match CString::new(out_json) {
                Ok(c_string) => Some(c_string.into_raw()),
                Err(e) => {
                    log::error!("run_verify_c: Failed to create C string from result: {}", e);
                    None
                }
            }
        },
        "run_verify_c",
    );

    result.unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn build_chain_decision_c(
    mode: *const c_char,
    dns_json: *const c_char,
    non_dns_json: *const c_char,
    combined_json: *const c_char,
) -> *mut c_char {
    // Initialize logger (safe to call multiple times due to try_init)
    init_logger();

    // Wrap entire function in panic catch to prevent panics crossing FFI boundary
    let result = catch_unwind_ffi(
        || {
            // Validate all pointers are non-null
            if mode.is_null() {
                log::error!("build_chain_decision_c: Called with null mode pointer");
                return None;
            }
            if dns_json.is_null() {
                log::error!("build_chain_decision_c: Called with null dns_json pointer");
                return None;
            }
            if non_dns_json.is_null() {
                log::error!("build_chain_decision_c: Called with null non_dns_json pointer");
                return None;
            }
            if combined_json.is_null() {
                log::error!("build_chain_decision_c: Called with null combined_json pointer");
                return None;
            }

            // Validate and parse mode string
            let mode_c_str = unsafe { CStr::from_ptr(mode) };
            let mode_str = match mode_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Invalid UTF-8 in mode parameter: {}",
                        e
                    );
                    return None;
                }
            };

            // Validate mode is one of the allowed values
            if !validate_mode_string(mode_str) {
                log::error!("build_chain_decision_c: Invalid mode parameter: '{}' (allowed: lite, stealth, high, phantom)", mode_str);
                return None;
            }

            // Validate and parse DNS JSON
            let dns_c_str = unsafe { CStr::from_ptr(dns_json) };
            let dns_str = match dns_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Invalid UTF-8 in dns_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(dns_str) {
                log::error!("build_chain_decision_c: Invalid dns_json array structure");
                return None;
            }

            // Validate and parse non-DNS JSON
            let non_dns_c_str = unsafe { CStr::from_ptr(non_dns_json) };
            let non_dns_str = match non_dns_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Invalid UTF-8 in non_dns_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(non_dns_str) {
                log::error!("build_chain_decision_c: Invalid non_dns_json array structure");
                return None;
            }

            // Validate and parse combined JSON
            let combined_c_str = unsafe { CStr::from_ptr(combined_json) };
            let combined_str = match combined_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Invalid UTF-8 in combined_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(combined_str) {
                log::error!("build_chain_decision_c: Invalid combined_json array structure");
                return None;
            }

            // Parse JSON arrays into Proxy objects
            let dns: Vec<types::Proxy> = match serde_json::from_str(dns_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_decision_c: Failed to parse dns_json: {} | Input preview: {:.100}", e, dns_str);
                    return None;
                }
            };

            let non_dns: Vec<types::Proxy> = match serde_json::from_str(non_dns_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_decision_c: Failed to parse non_dns_json: {} | Input preview: {:.100}", e, non_dns_str);
                    return None;
                }
            };

            let combined: Vec<types::Proxy> = match serde_json::from_str(combined_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_decision_c: Failed to parse combined_json: {} | Input preview: {:.100}", e, combined_str);
                    return None;
                }
            };

            // Validate proxy data in all arrays
            for (arrays_name, proxies) in [
                ("dns", &dns),
                ("non_dns", &non_dns),
                ("combined", &combined),
            ] {
                for (i, proxy) in proxies.iter().enumerate() {
                    if proxy.ip.is_empty() || proxy.ip.len() > 255 {
                        log::error!(
                            "build_chain_decision_c: Invalid IP in {} array at index {}",
                            arrays_name,
                            i
                        );
                        return None;
                    }
                    if proxy.port == 0 {
                        log::error!(
                            "build_chain_decision_c: Invalid port in {} array at index {}",
                            arrays_name,
                            i
                        );
                        return None;
                    }
                    if proxy.proto.is_empty() || proxy.proto.len() > 32 {
                        log::error!("build_chain_decision_c: Invalid protocol in {} array at index {}: IP={}, Port={}, Proto='{}'", 
                            arrays_name, i, proxy.ip, proxy.port, proxy.proto);
                        return None;
                    }
                }
            }

            // Build the chain decision
            let decision = match rotator::build_chain_decision(mode_str, &dns, &non_dns, &combined)
            {
                Some(d) => d,
                None => {
                    log::error!(
                        "build_chain_decision_c: build_chain_decision returned None for mode: {}",
                        mode_str
                    );
                    return None;
                }
            };

            let out_json = match serde_json::to_string(&decision) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Failed to serialize chain decision: {}",
                        e
                    );
                    return None;
                }
            };

            match CString::new(out_json) {
                Ok(c_string) => Some(c_string.into_raw()),
                Err(e) => {
                    log::error!(
                        "build_chain_decision_c: Failed to create C string from decision: {}",
                        e
                    );
                    None
                }
            }
        },
        "build_chain_decision_c",
    );

    // Return null pointer on panic or error
    result.unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn free_c_string(s: *mut c_char) {
    // Initialize logger (safe to call multiple times due to try_init)
    init_logger();

    // Wrap in panic catch to prevent panics crossing FFI boundary
    catch_unwind_ffi_void(
        || {
            if s.is_null() {
                log::warn!(
                    "free_c_string: Called with null pointer (potential double-free or misuse)"
                );
                return;
            }
            log::debug!("free_c_string: Successfully freeing C string at {:?}", s);
            unsafe {
                let _ = CString::from_raw(s);
            }
        },
        "free_c_string",
    );
}

/// C API function that returns chain topology WITHOUT encryption keys.
/// This is the secure version for persisting to disk (last_chain.json).
/// Keys remain only in memory and are never written to storage.
#[no_mangle]
pub extern "C" fn build_chain_topology_c(
    mode: *const c_char,
    dns_json: *const c_char,
    non_dns_json: *const c_char,
    combined_json: *const c_char,
) -> *mut c_char {
    // Initialize logger (safe to call multiple times due to try_init)
    init_logger();

    // Wrap entire function in panic catch to prevent panics crossing FFI boundary
    let result = catch_unwind_ffi(
        || {
            // Validate all pointers are non-null
            if mode.is_null() {
                log::error!("build_chain_topology_c: Called with null mode pointer");
                return None;
            }
            if dns_json.is_null() {
                log::error!("build_chain_topology_c: Called with null dns_json pointer");
                return None;
            }
            if non_dns_json.is_null() {
                log::error!("build_chain_topology_c: Called with null non_dns_json pointer");
                return None;
            }
            if combined_json.is_null() {
                log::error!("build_chain_topology_c: Called with null combined_json pointer");
                return None;
            }

            // Validate and parse mode string
            let mode_c_str = unsafe { CStr::from_ptr(mode) };
            let mode_str = match mode_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Invalid UTF-8 in mode parameter: {}",
                        e
                    );
                    return None;
                }
            };

            // Validate mode is one of the allowed values
            if !validate_mode_string(mode_str) {
                log::error!("build_chain_topology_c: Invalid mode parameter: '{}' (allowed: lite, stealth, high, phantom)", mode_str);
                return None;
            }

            // Validate and parse DNS JSON
            let dns_c_str = unsafe { CStr::from_ptr(dns_json) };
            let dns_str = match dns_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Invalid UTF-8 in dns_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(dns_str) {
                log::error!("build_chain_topology_c: Invalid dns_json array structure");
                return None;
            }

            // Validate and parse non-DNS JSON
            let non_dns_c_str = unsafe { CStr::from_ptr(non_dns_json) };
            let non_dns_str = match non_dns_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Invalid UTF-8 in non_dns_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(non_dns_str) {
                log::error!("build_chain_topology_c: Invalid non_dns_json array structure");
                return None;
            }

            // Validate and parse combined JSON
            let combined_c_str = unsafe { CStr::from_ptr(combined_json) };
            let combined_str = match combined_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Invalid UTF-8 in combined_json parameter: {}",
                        e
                    );
                    return None;
                }
            };

            if !validate_json_array(combined_str) {
                log::error!("build_chain_topology_c: Invalid combined_json array structure");
                return None;
            }

            // Parse JSON arrays into Proxy objects
            let dns: Vec<types::Proxy> = match serde_json::from_str(dns_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_topology_c: Failed to parse dns_json: {} | Input preview: {:.100}", e, dns_str);
                    return None;
                }
            };

            let non_dns: Vec<types::Proxy> = match serde_json::from_str(non_dns_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_topology_c: Failed to parse non_dns_json: {} | Input preview: {:.100}", e, non_dns_str);
                    return None;
                }
            };

            let combined: Vec<types::Proxy> = match serde_json::from_str(combined_str) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("build_chain_topology_c: Failed to parse combined_json: {} | Input preview: {:.100}", e, combined_str);
                    return None;
                }
            };

            // Validate proxy data in all arrays
            for (arrays_name, proxies) in [
                ("dns", &dns),
                ("non_dns", &non_dns),
                ("combined", &combined),
            ] {
                for (i, proxy) in proxies.iter().enumerate() {
                    if proxy.ip.is_empty() || proxy.ip.len() > 255 {
                        log::error!(
                            "build_chain_topology_c: Invalid IP in {} array at index {}",
                            arrays_name,
                            i
                        );
                        return None;
                    }
                    if proxy.port == 0 {
                        log::error!(
                            "build_chain_topology_c: Invalid port in {} array at index {}",
                            arrays_name,
                            i
                        );
                        return None;
                    }
                    if proxy.proto.is_empty() || proxy.proto.len() > 32 {
                        log::error!(
                            "build_chain_topology_c: Invalid protocol in {} array at index {}",
                            arrays_name,
                            i
                        );
                        return None;
                    }
                }
            }

            // Build the chain decision
            let decision = match rotator::build_chain_decision(mode_str, &dns, &non_dns, &combined)
            {
                Some(d) => d,
                None => {
                    log::error!(
                        "build_chain_topology_c: build_chain_decision returned None for mode: {}",
                        mode_str
                    );
                    return None;
                }
            };

            // SECURITY: Convert to topology-only format, stripping encryption keys
            let topology = decision.to_chain_topology();

            let out_json = match serde_json::to_string(&topology) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Failed to serialize chain topology: {}",
                        e
                    );
                    return None;
                }
            };

            match CString::new(out_json) {
                Ok(c_string) => Some(c_string.into_raw()),
                Err(e) => {
                    log::error!(
                        "build_chain_topology_c: Failed to create C string from topology: {}",
                        e
                    );
                    None
                }
            }
        },
        "build_chain_topology_c",
    );

    // Return null pointer on panic or error
    result.unwrap_or(std::ptr::null_mut())
}

/// C API function to derive encryption keys from a master secret.
/// Returns JSON array of {key_hex, nonce_hex} objects for each hop.
/// This allows regenerating keys without storing them.
#[no_mangle]
pub extern "C" fn derive_keys_from_secret_c(
    master_secret: *const c_char,
    chain_id: *const c_char,
    num_hops: usize,
) -> *mut c_char {
    // Initialize logger (safe to call multiple times due to try_init)
    init_logger();

    // Wrap entire function in panic catch to prevent panics crossing FFI boundary
    let result = catch_unwind_ffi(
        || {
            if master_secret.is_null() || chain_id.is_null() {
                log::error!("derive_keys_from_secret_c: Called with null pointer (master_secret or chain_id)");
                return None;
            }

            let secret_c_str = unsafe { CStr::from_ptr(master_secret) };
            let secret_str = match secret_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "derive_keys_from_secret_c: Invalid UTF-8 in master_secret: {}",
                        e
                    );
                    return None;
                }
            };

            let chain_id_c_str = unsafe { CStr::from_ptr(chain_id) };
            let chain_id_str = match chain_id_c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "derive_keys_from_secret_c: Invalid UTF-8 in chain_id: {}",
                        e
                    );
                    return None;
                }
            };

            if num_hops == 0 || num_hops > 100 {
                log::error!(
                    "derive_keys_from_secret_c: Invalid num_hops: {} (must be 1-100)",
                    num_hops
                );
                return None;
            }

            let encryption = rotator::generate_encryption_from_secret(
                secret_str.as_bytes(),
                chain_id_str,
                num_hops,
            );

            let out_json = match serde_json::to_string(&encryption) {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "derive_keys_from_secret_c: Failed to serialize encryption keys: {}",
                        e
                    );
                    return None;
                }
            };

            match CString::new(out_json) {
                Ok(c_string) => Some(c_string.into_raw()),
                Err(e) => {
                    log::error!(
                        "derive_keys_from_secret_c: Failed to create C string from keys: {}",
                        e
                    );
                    None
                }
            }
        },
        "derive_keys_from_secret_c",
    );

    // Return null pointer on panic or error
    result.unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn start_spectre_server_c(
    port: u16,
    decision_json: *const c_char,
    dns_json: *const c_char,
    non_dns_json: *const c_char,
    combined_json: *const c_char,
) -> i32 {
    init_logger();

    let result = catch_unwind_ffi(
        || {
            if decision_json.is_null() {
                log::error!("start_spectre_server_c: decision_json is null");
                return Some(-1);
            }
            if dns_json.is_null() || non_dns_json.is_null() || combined_json.is_null() {
                log::error!("start_spectre_server_c: missing pool pointers");
                return Some(-10);
            }

            let c_str = unsafe { CStr::from_ptr(decision_json) };
            let json_str = match c_str.to_str() {
                Ok(s) => s,
                Err(e) => {
                    log::error!(
                        "start_spectre_server_c: Invalid UTF-8 in decision_json: {}",
                        e
                    );
                    return Some(-2);
                }
            };

            let decision: types::RotationDecision = match serde_json::from_str(json_str) {
                Ok(d) => d,
                Err(e) => {
                    log::error!(
                        "start_spectre_server_c: Failed to parse decision JSON: {}",
                        e
                    );
                    return Some(-3);
                }
            };

            // Parse pools for live rotation
            let dns: Vec<types::Proxy> = match serde_json::from_str(unsafe {
                CStr::from_ptr(dns_json).to_str().unwrap_or("[]")
            }) {
                Ok(p) => p,
                _ => Vec::new(),
            };
            let non_dns: Vec<types::Proxy> = match serde_json::from_str(unsafe {
                CStr::from_ptr(non_dns_json).to_str().unwrap_or("[]")
            }) {
                Ok(p) => p,
                _ => Vec::new(),
            };
            let combined: Vec<types::Proxy> = match serde_json::from_str(unsafe {
                CStr::from_ptr(combined_json).to_str().unwrap_or("[]")
            }) {
                Ok(p) => p,
                _ => Vec::new(),
            };

            // Start tokio runtime and block on the server (this blocks the C caller thread)
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    log::error!(
                        "start_spectre_server_c: Failed to build tokio runtime: {}",
                        e
                    );
                    return Some(-4);
                }
            };

            match rt.block_on(tunnel::start_socks_server(
                port, decision, dns, non_dns, combined,
            )) {
                Ok(_) => Some(0),
                Err(e) => {
                    log::error!("start_spectre_server_c: Server error: {}", e);
                    Some(-5)
                }
            }
        },
        "start_spectre_server_c",
    );

    result.unwrap_or(-99)
}
