#!/usr/bin/env node
/**
 * Calls GET /health against the control plane (same contract as CLI `health`).
 * Uses CONTROL_PLANE_URL or REMOTE_HARNESS_URL, default http://127.0.0.1:3000
 */
const base = (
  process.env.CONTROL_PLANE_URL ||
  process.env.REMOTE_HARNESS_URL ||
  "http://127.0.0.1:3000"
).replace(/\/$/, "");
const url = `${base}/health`;

const res = await fetch(url);
if (!res.ok) {
  console.error(`HTTP ${res.status} from ${url}`);
  process.exit(1);
}
const body = await res.json();
if (body?.status !== "ok") {
  console.error("Unexpected body:", body);
  process.exit(1);
}
console.log(`control plane healthy (${url})`);
