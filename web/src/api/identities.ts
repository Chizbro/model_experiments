import { controlPlaneJson, controlPlaneNoContent } from "./client";
import type { IdentityCredentials } from "./types";

export async function getIdentityCredentials(
  baseUrl: string,
  apiKey: string,
  identityId: string,
): Promise<IdentityCredentials> {
  return controlPlaneJson<IdentityCredentials>({
    baseUrl,
    path: `/identities/${encodeURIComponent(identityId)}`,
    method: "GET",
    apiKey,
  });
}

/** `PATCH /identities/:id` — at least one of `agent_token` / `git_token` must be non-empty. */
export async function patchIdentityTokens(
  baseUrl: string,
  apiKey: string,
  identityId: string,
  patch: { agent_token?: string; git_token?: string },
): Promise<void> {
  const jsonBody: Record<string, string> = {};
  const a = patch.agent_token?.trim();
  const g = patch.git_token?.trim();
  if (a) {
    jsonBody.agent_token = a;
  }
  if (g) {
    jsonBody.git_token = g;
  }
  if (Object.keys(jsonBody).length === 0) {
    throw new Error("Enter a token to save.");
  }
  await controlPlaneNoContent({
    baseUrl,
    path: `/identities/${encodeURIComponent(identityId)}`,
    method: "PATCH",
    apiKey,
    jsonBody,
  });
}
