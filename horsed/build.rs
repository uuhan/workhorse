use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let repo_dir = manifest_dir
        .parent()
        .map_or(manifest_dir.clone(), PathBuf::from);

    let git_head = repo_dir.join(".git").join("HEAD");
    println!("cargo:rerun-if-changed={}", git_head.display());

    let commit = Command::new("git")
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
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=HORSED_GIT_SHA={commit}");
}
