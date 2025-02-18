use std::env;
use std::process::Command;

fn main() {
    // Set build timestamp
    let timestamp = chrono::Utc::now().to_rfc3339();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);

    // Get Rust version
    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        if let Ok(version) = String::from_utf8(output.stdout) {
            println!("cargo:rustc-env=RUST_VERSION={}", version.trim());
        }
    }

    // Get Git commit hash
    if let Ok(output) = Command::new("git").args(&["rev-parse", "HEAD"]).output() {
        if let Ok(hash) = String::from_utf8(output.stdout) {
            println!("cargo:rustc-env=GIT_HASH={}", hash.trim());
        }
    }
}
