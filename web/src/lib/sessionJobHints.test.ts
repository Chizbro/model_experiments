import { describe, expect, it } from "vitest";
import {
  commitPushOutcomeHint,
  failedJobRemoteCommitHint,
  pullRequestExpectationHint,
  sessionJobOutcomeNotes,
} from "./sessionJobHints";

const baseJob = { job_id: "j", status: "completed", created_at: "t" } as const;

describe("pullRequestExpectationHint", () => {
  it("returns null when branch_mode is not pr", () => {
    expect(
      pullRequestExpectationHint(
        { ...baseJob, pull_request_url: null },
        { branch_mode: "main" },
      ),
    ).toBeNull();
  });

  it("returns a line when PR expected but missing on completed job", () => {
    expect(
      pullRequestExpectationHint(
        { ...baseJob, pull_request_url: null },
        { branch_mode: "pr" },
      ),
    ).toMatchInlineSnapshot(
      `"PR/MR was not created: check branch/title params, provider (GitHub/GitLab), and token scopes — see Architecture §9b."`,
    );
  });
});

describe("commitPushOutcomeHint", () => {
  it("flags missing commit when git outcomes expected", () => {
    expect(
      commitPushOutcomeHint(
        { ...baseJob, commit_ref: null, pull_request_url: null },
        { branch_mode: "main" },
      ),
    ).toMatchInlineSnapshot(
      `"No commit was recorded for this job—push may not have completed; check logs and error_message (CLIENT_EXPERIENCE §8.1)."`,
    );
  });

  it("stays silent when commit exists", () => {
    expect(
      commitPushOutcomeHint(
        { ...baseJob, commit_ref: "abc123", pull_request_url: null },
        { branch_mode: "main" },
      ),
    ).toBeNull();
  });
});

describe("failedJobRemoteCommitHint", () => {
  it("explains failed status vs remote commit", () => {
    expect(
      failedJobRemoteCommitHint({
        ...baseJob,
        status: "failed",
        commit_ref: "deadbeef",
        pull_request_url: null,
      }),
    ).toMatchInlineSnapshot(
      `"Job status is failed, but a commit may exist on the remote from this attempt; a PR/MR may be skipped when the run did not succeed (Architecture §9a)."`,
    );
  });
});

describe("sessionJobOutcomeNotes (error-state snapshots)", () => {
  it("PR mode completed without PR", () => {
    expect(
      sessionJobOutcomeNotes({ ...baseJob, pull_request_url: null, commit_ref: "aaa" }, { branch_mode: "pr" }),
    ).toMatchInlineSnapshot(
      `"PR/MR was not created: check branch/title params, provider (GitHub/GitLab), and token scopes — see Architecture §9b."`,
    );
  });

  it("failed job with commit ref (PR mode)", () => {
    expect(
      sessionJobOutcomeNotes(
        {
          ...baseJob,
          status: "failed",
          commit_ref: "bbb",
          error_message: "agent exited 1",
          pull_request_url: null,
        },
        { branch_mode: "pr" },
      ),
    ).toMatchInlineSnapshot(
      `"PR/MR was not created: job did not complete successfully. Job status is failed, but a commit may exist on the remote from this attempt; a PR/MR may be skipped when the run did not succeed (Architecture §9a)."`,
    );
  });
});
