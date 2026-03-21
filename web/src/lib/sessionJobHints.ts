import type { SessionJobDetail } from "../api/types";

/** Session params imply the user expected Git push / PR-style outcomes. */
export function expectsGitOutcomes(params: Record<string, unknown>): boolean {
  const bm = params.branch_mode;
  if (bm === "pr" || bm === "main") return true;
  const p = params.branch_name_prefix;
  return typeof p === "string" && p.trim().length > 0;
}

/** One-line hint when the user expected a PR/MR link but none is present (CLIENT_EXPERIENCE §8.1). */
export function pullRequestExpectationHint(
  job: SessionJobDetail,
  params: Record<string, unknown>,
): string | null {
  const branchMode = params.branch_mode;
  const wantsPr = branchMode === "pr";
  if (!wantsPr) return null;
  if (job.pull_request_url) return null;
  if (job.status !== "completed") {
    return "PR/MR was not created: job did not complete successfully.";
  }
  if (job.error_message) {
    return "PR/MR was not created: see job error above.";
  }
  return "PR/MR was not created: check branch/title params, provider (GitHub/GitLab), and token scopes — see Architecture §9b.";
}

/** Completed job but no recorded commit when Git outcomes were expected (CLIENT_EXPERIENCE §8.1). */
export function commitPushOutcomeHint(
  job: SessionJobDetail,
  params: Record<string, unknown>,
): string | null {
  if (job.status !== "completed") return null;
  if (job.commit_ref?.trim()) return null;
  if (!expectsGitOutcomes(params)) return null;
  return "No commit was recorded for this job—push may not have completed; check logs and error_message (CLIENT_EXPERIENCE §8.1).";
}

/** Failed job that may still have left commits on the remote (Architecture §9a / CLIENT_EXPERIENCE §8.1). */
export function failedJobRemoteCommitHint(job: SessionJobDetail): string | null {
  if (job.status !== "failed") return null;
  if (!job.commit_ref?.trim()) return null;
  return "Job status is failed, but a commit may exist on the remote from this attempt; a PR/MR may be skipped when the run did not succeed (Architecture §9a).";
}

/** Combined Note column: PR expectation, missing commit, and failed-vs-Git distinction. */
export function sessionJobOutcomeNotes(job: SessionJobDetail, params: Record<string, unknown>): string {
  const parts: string[] = [];
  const pr = pullRequestExpectationHint(job, params);
  if (pr) parts.push(pr);
  const commit = commitPushOutcomeHint(job, params);
  if (commit) parts.push(commit);
  const fail = failedJobRemoteCommitHint(job);
  if (fail) parts.push(fail);
  return parts.join(" ");
}
