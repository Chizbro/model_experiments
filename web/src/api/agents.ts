import { controlPlaneJson } from "./client";
import type { Paginated } from "./types";

export interface PostAgentInboxBody {
  payload: Record<string, unknown>;
  persona_id?: string;
}

export interface PostAgentInboxResponse {
  task_id: string;
}

export interface InboxTaskItem {
  task_id: string;
  payload: Record<string, unknown>;
  enqueued_at: string;
}

function inboxListQueryString(opts: { limit?: number; cursor?: string | null }): string {
  const q = new URLSearchParams();
  if (opts.limit != null) q.set("limit", String(opts.limit));
  if (opts.cursor) q.set("cursor", opts.cursor);
  const s = q.toString();
  return s ? `?${s}` : "";
}

export async function postAgentInbox(
  baseUrl: string,
  apiKey: string,
  agentId: string,
  body: PostAgentInboxBody,
): Promise<PostAgentInboxResponse> {
  const id = encodeURIComponent(agentId.trim());
  return controlPlaneJson<PostAgentInboxResponse>({
    baseUrl,
    path: `/agents/${id}/inbox`,
    method: "POST",
    apiKey,
    jsonBody: body,
  });
}

export async function listAgentInboxPage(
  baseUrl: string,
  apiKey: string,
  agentId: string,
  opts?: { limit?: number; cursor?: string | null },
): Promise<Paginated<InboxTaskItem>> {
  const id = encodeURIComponent(agentId.trim());
  return controlPlaneJson<Paginated<InboxTaskItem>>({
    baseUrl,
    path: `/agents/${id}/inbox${inboxListQueryString({ limit: opts?.limit ?? 20, cursor: opts?.cursor })}`,
    method: "GET",
    apiKey,
  });
}
