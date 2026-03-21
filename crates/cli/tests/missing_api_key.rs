use std::path::PathBuf;
use std::process::Command;

fn remote_harness_bin() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    manifest
        .join("../..")
        .join("target")
        .join(profile)
        .join("remote-harness")
}

#[test]
fn session_list_without_api_key_exits_nonzero_with_stderr_hint() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = remote_harness_bin();
    let out = Command::new(&bin)
        .args(["session", "list"])
        .env("HOME", tmp.path())
        .env_remove("REMOTE_HARNESS_API_KEY")
        .env_remove("API_KEY")
        .output()
        .expect("spawn cli");
    assert!(
        !out.status.success(),
        "expected failure, stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("API key"),
        "expected API key hint on stderr, got: {stderr:?}"
    );
}
