//! Runtime platform detection (macOS, Linux, WSL, Windows) for spawn policy and worker labels.

/// Fine-grained platform for CLI invocation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPlatform {
    Macos,
    Linux,
    /// Linux kernel under WSL1/WSL2 — same process model as Linux; labeled separately for ops.
    Wsl,
    Windows,
}

/// Detect the current worker platform. WSL is reported as [`WorkerPlatform::Wsl`] when detected.
pub fn detect_worker_platform() -> WorkerPlatform {
    #[cfg(target_os = "windows")]
    {
        WorkerPlatform::Windows
    }
    #[cfg(all(not(target_os = "windows"), target_os = "macos"))]
    {
        WorkerPlatform::Macos
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        target_os = "linux"
    ))]
    {
        if is_wsl() {
            WorkerPlatform::Wsl
        } else {
            WorkerPlatform::Linux
        }
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        not(target_os = "linux")
    ))]
    {
        WorkerPlatform::Linux
    }
}

#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| {
            let l = s.to_lowercase();
            l.contains("microsoft") || l.contains("wsl")
        })
        .unwrap_or(false)
}

/// Value for `labels.platform` on worker register ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §4c).
pub fn register_platform_label() -> &'static str {
    match detect_worker_platform() {
        WorkerPlatform::Macos => "macos",
        WorkerPlatform::Linux => "linux",
        WorkerPlatform::Wsl => "wsl",
        WorkerPlatform::Windows => "windows",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_matches_detection() {
        let p = detect_worker_platform();
        let label = register_platform_label();
        match p {
            WorkerPlatform::Macos => assert_eq!(label, "macos"),
            WorkerPlatform::Linux => assert_eq!(label, "linux"),
            WorkerPlatform::Wsl => assert_eq!(label, "wsl"),
            WorkerPlatform::Windows => assert_eq!(label, "windows"),
        }
    }
}
