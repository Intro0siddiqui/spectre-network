use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::ffi::{CStr, CString};
use std::fs;
use std::io;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

pub mod crypto;
pub mod polish;
pub mod rotator;
pub mod types;
pub mod verifier;

use types::Proxy;

// Helper to load files
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

fn load_all_pools(workspace: &Path) -> io::Result<(Vec<Proxy>, Vec<Proxy>, Vec<Proxy>)> {
    let dns = load_json_array(&workspace.join("proxies_dns.json"))?;
    let non_dns = load_json_array(&workspace.join("proxies_non_dns.json"))?;
    let combined = load_json_array(&workspace.join("proxies_combined.json"))?;
    Ok((dns, non_dns, combined))
}

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

#[pyfunction]
fn version() -> PyResult<String> {
    Ok("rotator_rs_pyo3_v1".to_string())
}

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

#[no_mangle]
pub extern "C" fn run_polish_c(raw_json: *const c_char) -> *mut c_char {
    if raw_json.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = unsafe { CStr::from_ptr(raw_json) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let proxies: Vec<types::Proxy> = match serde_json::from_str(json_str) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };

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
        Err(_) => return std::ptr::null_mut(),
    };

    CString::new(out_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn build_chain_decision_c(
    mode: *const c_char,
    dns_json: *const c_char,
    non_dns_json: *const c_char,
    combined_json: *const c_char,
) -> *mut c_char {
    if mode.is_null() || dns_json.is_null() || non_dns_json.is_null() || combined_json.is_null() {
        return std::ptr::null_mut();
    }

    let mode_str = unsafe { CStr::from_ptr(mode) }.to_str().unwrap_or("");
    let dns_str = unsafe { CStr::from_ptr(dns_json) }.to_str().unwrap_or("[]");
    let non_dns_str = unsafe { CStr::from_ptr(non_dns_json) }
        .to_str()
        .unwrap_or("[]");
    let combined_str = unsafe { CStr::from_ptr(combined_json) }
        .to_str()
        .unwrap_or("[]");

    let dns: Vec<types::Proxy> = serde_json::from_str(dns_str).unwrap_or_default();
    let non_dns: Vec<types::Proxy> = serde_json::from_str(non_dns_str).unwrap_or_default();
    let combined: Vec<types::Proxy> = serde_json::from_str(combined_str).unwrap_or_default();

    let decision = match rotator::build_chain_decision(mode_str, &dns, &non_dns, &combined) {
        Some(d) => d,
        None => return std::ptr::null_mut(),
    };

    let out_json = match serde_json::to_string(&decision) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    CString::new(out_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn free_c_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}
