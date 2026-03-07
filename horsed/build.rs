use std::env;
use std::path::PathBuf;
use std::process::Command;

fn normalize_commit(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    // Prefer a short hash for compact health output.
    let hex: String = value
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .take(12)
        .collect();
    if !hex.is_empty() {
        return Some(hex);
    }

    Some(value.to_string())
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let repo_dir = manifest_dir
        .parent()
        .map_or(manifest_dir.clone(), PathBuf::from);

    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=HORSED_GIT_SHA");

    let git_head = repo_dir.join(".git").join("HEAD");
    println!("cargo:rerun-if-changed={}", git_head.display());

    let commit = env::var("HORSED_GIT_SHA")
        .ok()
        .as_deref()
        .and_then(normalize_commit)
        .or_else(|| {
            env::var("GIT_COMMIT")
                .ok()
                .as_deref()
                .and_then(normalize_commit)
        })
        .or_else(|| {
            Command::new("git")
                .arg("rev-parse")
                .arg("--short=12")
                .arg("HEAD")
                .current_dir(&repo_dir)
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .as_deref()
                .and_then(normalize_commit)
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=HORSED_GIT_SHA={commit}");
}
