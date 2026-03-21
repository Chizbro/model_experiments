//! OpenAPI contract checks: valid YAML, version, and operationId allowlist.
//!
//! When you add or remove REST operations in `openapi.yaml`, update
//! `EXPECTED_OPERATION_IDS` so CI catches edits that skip the paired Rust update.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const EXPECTED_OPERATION_IDS: &[&str] = &[
    "bootstrapApiKey",
    "completeWorkerTask",
    "createApiKey",
    "createSession",
    "deleteApiKey",
    "deleteSession",
    "deleteSessionLogs",
    "deleteWorker",
    "getAgentInbox",
    "getHealth",
    "getHealthIdle",
    "getIdentity",
    "getIdentityAuthStatus",
    "getReady",
    "getSession",
    "getWorker",
    "githubOauthCallback",
    "gitlabOauthCallback",
    "heartbeatWorker",
    "listApiKeys",
    "listIdentityRepositories",
    "listSessionLogs",
    "listSessions",
    "listWorkers",
    "patchIdentity",
    "patchSession",
    "patchSessionJob",
    "postAgentInbox",
    "postWorkerInboxListener",
    "postWorkerTaskLogs",
    "pullTask",
    "registerWorker",
    "sendSessionInput",
    "startGithubOauth",
    "startGitlabOauth",
    "streamSessionEvents",
    "streamSessionLogs",
];

fn openapi_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("openapi.yaml")
}

fn collect_operation_ids(doc: &serde_yaml::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(paths) = doc.get("paths").and_then(|p| p.as_mapping()) else {
        return out;
    };
    for (_, methods) in paths {
        let Some(methods) = methods.as_mapping() else {
            continue;
        };
        for (key, op) in methods {
            let Some(key_str) = key.as_str() else {
                continue;
            };
            if !matches!(
                key_str,
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace"
            ) {
                continue;
            }
            if let Some(id) = op.get("operationId").and_then(|v| v.as_str()) {
                out.insert(id.to_string());
            }
        }
    }
    out
}

#[test]
fn openapi_yaml_parseable_and_operation_ids_match_allowlist() {
    let raw = fs::read_to_string(openapi_path()).expect("read openapi.yaml");
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw).expect("openapi.yaml must parse");

    let ver = doc
        .get("openapi")
        .and_then(|v| v.as_str())
        .expect("openapi field");
    assert!(ver.starts_with("3."), "expected OpenAPI 3.x, got {ver:?}");

    let found = collect_operation_ids(&doc);
    let expected: BTreeSet<String> = EXPECTED_OPERATION_IDS
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    assert_eq!(
        found, expected,
        "openapi.yaml operationIds must match EXPECTED_OPERATION_IDS in openapi_contract.rs.\n\
         Found: {found:?}\n\
         Expected: {expected:?}\n\
         After changing the spec, update the allowlist (and implement the route)."
    );
}
