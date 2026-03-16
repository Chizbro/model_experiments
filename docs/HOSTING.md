# Hosting and Deployment Flexibility

The harness is designed to run in a variety of environments. No single hosting model is mandatory: you can run the control plane and workers on always-on servers, on sleepable desktops and laptops, or in the cloud. This doc describes **deployment topologies** and how to support **power-saving setups** (e.g. control plane and workers on machines that can sleep) without locking others into that architecture.

---

## 1. Design principle

- **Flexible topologies:** The control plane (with its DB) and workers can run on one machine or many; the only requirement is that workers can reach the control plane and Git remotes when they are running.
- **No mandated “always on”:** Some deployments will run the control plane on an always-on server; others will run it on a desktop or laptop that can be powered off or put to sleep. The product should work in both cases.
- **Wake-up is an integration, not a built-in:** If you want the UI to “wake” a sleeping backend, that is done via a **configurable integration** (e.g. a URL or script you provide). The harness does not implement Wake-on-LAN or any specific wake mechanism; it provides a hook so your setup can.

---

## 2. Standard topology (always-on)

- **Control plane** (API + engine + session store + task queue + log aggregator) and **PostgreSQL** run on one or more hosts that are assumed reachable whenever you use the system.
- **Workers** run on one or more machines (same host or different); they register when they start and pull tasks while running.
- **UI** is typically served by the control plane or a reverse proxy in front of it. **CLI** talks to the control plane URL.

No special handling for sleep or power-off; everything is “on” when in use.

---

## 3. Power-saving / sleepable topology

In this model, the **control plane and workers can run on machines that are powered off or put to sleep** to save power. The backend (and thus the database) lives on one of those machines—no requirement for a separate, always-on hosted database.

**Example setup (yours):**

- **Machine A:** Windows desktop (optionally WSL) — runs control plane + PostgreSQL, and/or a worker. Can be off or asleep.
- **Machine B:** Mac laptop — runs a worker. Can be off or asleep.
- **Always-on host:** A separate, low-power device (e.g. Raspberry Pi, NAS, or small VPS) that:
  - Is reachable when you’re away (e.g. via **Tailscale**).
  - Can send **Wake-on-LAN (WOL)** packets to Machine A and/or B on your local network.
  - Optionally serves a minimal **“wake portal”** (see below).

When you want to use Remote Harness, you first wake the machine(s) that run the control plane (and workers if needed); once they’re up, you use the UI or CLI as normal.

**Design implications:**

- **Control plane + DB on one of your machines:** Supported. Run the server binary and Postgres (or SQLite for minimal setups) on the Windows PC or Mac. When that machine sleeps, the harness is unavailable until it’s woken.
- **Workers on Windows and macOS:** The worker binary is built for these platforms (and Linux). Windows can use native or WSL; the worker runs in that environment and uses the same control-plane URL and auth.
- **No lock-in:** This topology is one valid way to run the harness. Others can run control plane and workers on always-on servers or in the cloud; the product does not assume sleepable hosts.

---

## 4. Wake integration (optional, deployer-provided)

To support “the UI can wake the backend,” the harness exposes a **configurable wake path** that **you** implement using your own always-on host and WOL (or similar).

**Contract:**

- The **UI** (and optionally the CLI) can show a “Service unavailable; it may be sleeping” state when the control plane is unreachable.
- If configured, a **“Wake up”** action (e.g. button in the UI or CLI command) triggers a **configurable request**: the client calls a **wake URL** (or runs a **wake script**) that you provide. The harness does not send WOL itself; it just invokes your URL or script.
- **You** run a small service on your always-on host that:
  - Listens on that URL (e.g. `https://always-on.example/wake-harness` or a Tailscale-only URL).
  - When called, sends WOL packets to the machine(s) that run the control plane (and optionally workers).
  - Returns success so the UI can show “Waking… check back in a minute” or poll until the control plane is reachable.

**Configuration:**

- In config or environment: **`WAKE_URL`** (e.g. `https://your-always-on-host/wake-harness`) and/or **`WAKE_SCRIPT`** (e.g. `/path/to/wake.sh`). If both are set, **WAKE_URL wins**—only the URL is invoked (HTTP request). If only WAKE_SCRIPT is set, the CLI runs that script **on the machine where the CLI is running** (local path; CLI process spawns it). No remote execution. If unset, no “Wake up” action is shown. See [Decisions §22](DECISIONS.md#22-wake-config-precedence-and-cli-script).
- This keeps the harness agnostic to Tailscale, WOL, or any specific tool; you plug in whatever you use.

**Optional: wake portal on the always-on host**

- You can serve a minimal static page or app on the always-on host (e.g. “Remote Harness is sleeping” + “Wake up” button that calls your WOL sender, then redirects to the control plane UI once it’s reachable). That portal is **your** deployment artifact, not part of the harness core. The harness only needs to support the configurable wake URL/script used by the main UI or that portal.

---

## 4b. UI hosting: public URL, client-only API access

When the control plane is reachable only on Tailscale (or your private network), the **Web UI can still be hosted publicly** (e.g. on your always-on host, a CDN, or any public URL). Access to the *application* remains restricted: only clients that can reach the control plane (e.g. browsers on devices that are on Tailscale) can actually use the app, because the browser must call the control plane API and WebSocket directly.

**Implication for the UI stack:** All communication with the control plane must happen **from the browser**. The UI must not use **server-side code** in the chosen framework to proxy requests to the control plane or to fetch API data on the server (e.g. Next.js server components or Nuxt server routes that call the control plane). If it did, that UI server would need to be on Tailscale (or the same network) to reach the control plane, which would defeat hosting the UI on a separate “public” host. So:

- The Web UI should be built as a **client-side application** (SPA): the browser loads the app, then the browser opens REST and WebSocket connections directly to the control plane URL (which may be a Tailscale URL when the user is remote).
- The UI framework should be used in a **client-only** way for control plane communication: client-side data fetching (e.g. TanStack Query in the browser), client-side WebSocket, no server-side proxy or server-rendered API calls to the control plane. Static export or a build that serves only static assets plus client-side JS is a good fit.
- This is noted so that framework choice (e.g. Vite + React, not Next.js with server components that hit the API) and deployment stay consistent with “UI hosted anywhere, control plane only on Tailscale.”

---

## 5. Platform and environment support

To support control plane and workers on Windows (including WSL), macOS, and Linux:

- **Control plane:** Build and run on Windows (native or WSL), macOS, Linux. PostgreSQL or SQLite (for minimal/local) on the same machine or a reachable host.
- **Workers:** Same platforms; worker binary for each (e.g. `remote-harness-worker` for Windows, macOS, Linux). On Windows, WSL is optional (native Windows or WSL both valid).
- **CLI:** Same platforms; user runs it from any machine that can reach the control plane URL (e.g. via Tailscale when away).
- **Web UI:** Served by the control plane (or a reverse proxy). When the control plane is asleep, the UI is unreachable unless you use a separate wake portal (above) on the always-on host.

This gives you the flexibility to run the backend on your Windows PC, workers on the PC and Mac laptop, and use an always-on host only for wake (and optionally a small wake portal), without forcing that model on other deployments.

---

## 6. Summary

| Aspect | Your (power-saving) setup | Others |
|--------|---------------------------|--------|
| Control plane + DB | On one of your machines (Windows or Mac); can sleep | Can be always-on server, cloud, or same as you |
| Workers | Windows PC (possibly WSL), Mac laptop; can sleep | Any reachable hosts |
| Wake-up | Optional: wake URL/script → your always-on host → WOL | Not needed if always on |
| Connectivity | Tailscale (your choice) for remote access | Any network/VPN they prefer |
| UI when backend asleep | Unreachable unless you run a wake portal on always-on host | N/A or same |

The product remains topology-agnostic: it works with always-on deployments, sleepable deployments, and mixed setups. The only addition is an **optional, configurable wake integration** so that deployments like yours can offer a “Wake up” flow without the harness depending on WOL, Tailscale, or a specific always-on host.

---

*See also: [Architecture](ARCHITECTURE.md) (logical topology), [Tech Stack](TECH_STACK.md) (platform support), [Decisions](DECISIONS.md) §8a (optional wake integration).*
