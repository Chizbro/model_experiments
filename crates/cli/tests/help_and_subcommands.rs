//! CLI help links to repo docs (plan task 21 acceptance).

use assert_cmd::Command;
use predicates::str::contains;

fn rh() -> Command {
    Command::cargo_bin("remote-harness").expect("remote-harness binary must be built for tests")
}

#[test]
fn root_long_help_lists_normative_doc_paths() {
    rh().arg("--help")
        .assert()
        .success()
        .stdout(contains("docs/API_OVERVIEW.md"))
        .stdout(contains("docs/SSE_EVENTS.md"))
        .stdout(contains("docs/TECH_STACK.md"));
}

#[test]
fn logs_tail_help_mentions_api_overview_section() {
    rh().args(["logs", "tail", "--help"])
        .assert()
        .success()
        .stdout(contains("API_OVERVIEW.md"));
}

#[test]
fn attach_help_mentions_events_and_logs_stream() {
    rh().args(["attach", "--help"])
        .assert()
        .success()
        .stdout(contains("/events"))
        .stdout(contains("logs/stream"));
}

#[test]
fn workers_alias_invokes_same_as_worker_list_help() {
    rh().args(["workers", "list", "--help"]).assert().success();
}

#[test]
fn session_start_alias_parseable() {
    rh().args([
        "session",
        "start",
        "https://github.com/x/y.git",
        "--prompt",
        "p",
        "--agent-cli",
        "cursor",
        "--help",
    ])
    .assert()
    .success();
}
