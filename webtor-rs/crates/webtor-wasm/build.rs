//! Build script for webtor-wasm
//!
//! Extracts version information from Cargo.lock at build time.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let lock_content = fs::read_to_string("../../Cargo.lock").unwrap_or_default();

    // Extract versions from Cargo.lock
    let subtle_tls_version = extract_version_from_lock(&lock_content, "subtle-tls").unwrap();
    let tor_proto_version = extract_version_from_lock(&lock_content, "tor-proto").unwrap();

    // Derive arti version from tor-proto (arti_minor = tor_proto_minor - 29)
    let arti_version = derive_arti_version(&tor_proto_version).unwrap();

    println!("cargo:rustc-env=SUBTLE_TLS_VERSION={}", subtle_tls_version);
    println!("cargo:rustc-env=TOR_PROTO_VERSION={}", tor_proto_version);
    println!("cargo:rustc-env=ARTI_VERSION={}", arti_version);
}

/// Extract package version from Cargo.lock content.
/// Cargo.lock format:
/// ```
/// [[package]]
/// name = "package-name"
/// version = "1.2.3"
/// ```
fn extract_version_from_lock(content: &str, package_name: &str) -> Option<String> {
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "[[package]]" {
            // Look for name and version in the next few lines
            let mut name = None;
            let mut version = None;
            for _ in 0..5 {
                if let Some(pkg_line) = lines.next() {
                    let pkg_line = pkg_line.trim();
                    if pkg_line.starts_with("name = ") {
                        name = pkg_line
                            .strip_prefix("name = ")
                            .map(|s| s.trim_matches('"').to_string());
                    } else if pkg_line.starts_with("version = ") {
                        version = pkg_line
                            .strip_prefix("version = ")
                            .map(|s| s.trim_matches('"').to_string());
                    }
                    if name.is_some() && version.is_some() {
                        break;
                    }
                    if pkg_line.is_empty() || pkg_line.starts_with("[[") {
                        break;
                    }
                }
            }
            if name.as_deref() == Some(package_name) {
                return version;
            }
        }
    }
    None
}

fn derive_arti_version(tor_proto_version: &str) -> Option<String> {
    // tor-proto 0.38.0 corresponds to arti 1.9.0
    // The pattern is: arti_minor = tor_proto_minor - 29
    let parts: Vec<&str> = tor_proto_version.split('.').collect();
    if parts.len() >= 2 {
        if let Ok(minor) = parts[1].parse::<i32>() {
            let arti_minor = minor - 29;
            if arti_minor >= 0 {
                return Some(format!("1.{}.0", arti_minor));
            }
        }
    }
    None
}
