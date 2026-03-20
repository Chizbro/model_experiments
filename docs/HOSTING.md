# Hosting and Deployment Flexibility

The harness is designed to run in a variety of environments. No single hosting model is mandatory: you can run the control plane and workers on always-on servers, on sleepable desktops and laptops, or in the cloud. This doc describes **deployment topologies** and how to support **power-saving setups** (e.g. control plane and workers on machines that can sleep) without locking others into that architecture.

---

## 1. Design principle

- **Flexible topologies:** The control plane (with its DB) and workers can run on one machine or many; the only requirement is that workers can reach the control plane and Git remotes when they are running.
- **No mandated "always on":** Some deployments will run the control plane on an always-on server; others will run it on a desktop or laptop that can be powered off or put to sleep. The product should work in both cases.
- **Wake-up is an integration, not a built-in:** If you want the UI to "wake" a sleeping backend, that is done via a **configurable integration** (e.g. a URL or script you provide). The harness does not implement Wake-on-LAN or any specific wake mechanism; it provides a hook so your setup can.

---

## 2. Standard topology (always-on)

- **Control plane** (API + engine + session store + task queue + log aggregator) and **PostgreSQL** run on one or more hosts that are assumed reachable whenever you use the system.
- **Workers** run on one or more machines (same host or different); they register when they start and pull tasks while running.
- **UI** is typically served by the control plane or a reverse proxy in front of it. **CLI** talks to the control plane URL.

No special handling for sleep or power-off; everything is "on" when in use.

---

## 3. Power-saving / sleepable topology

In this model, the **control plane and workers can run on machines that are powered off or put to sleep** to save power. The backend (and thus the database) lives on one of those machines -- no requirement for a separate, always-on hosted database.

**Example setup (yours):**

- **Machine A:** Windows desktop (optionally WSL) -- runs control plane + PostgreSQL, and/or a worker. Can be off or asleep.
- **Machine B:** Mac laptop -- runs a worker. Can be off or asleep.
- **Always-on host:** A separate, low-power device (e.g. Raspberry Pi, NAS, or small VPS) that:
  - Is reachable when you're away (e.g. via **Tailscale**).
  - Can send **Wake-on-LAN (WOL)** packets to Machine A and/or B on your local network.
  - Optionally serves a minimal "wake portal" (see below).

When you want to use Remote Harness, you first wake the machine(s) that run the control plane (and workers if needed); once they're up, you use the UI or CLI as normal.

**Design implications:**

- **Control plane + DB on one of your machines:** Supported. Run the server binary and Postgres (or SQLite for minimal setups) on the Windows PC or Mac. When that machine sleeps, the harness is unavailable until it's woken.
- **Workers on Windows and macOS:** The worker binary is built for these platforms (and Linux). Windows can use native or WSL; the worker runs in that environment and uses the same control-plane URL and auth.
- **No lock-in:** This topology is one valid way to run the harness. Others can run control plane and workers on always-on servers or in the cloud; the product does not assume sleepable hosts.

---

## 4. Wake integration (optional, deployer-provided)

To support "the UI can wake the backend," the harness exposes a **configurable wake path** that **you** implement using your own always-on host and WOL (or similar).

**Contract:**

- The **UI** (and optionally the CLI) can show a "Service unavailable; it may be sleeping" state when the control plane is unreachable.
- If configured, a **"Wake up"** action (e.g. button in the UI or CLI command) triggers a **configurable request**: the client calls a **wake URL** (or runs a **wake script**) that you provide. The harness does not send WOL itself; it just invokes your URL or script.
- **You** run a small service on your always-on host that:
  - Listens on that URL (e.g. `https://always-on.example/wake-harness` or a Tailscale-only URL).
  - When called, sends WOL packets to the machine(s) that run the control plane (and optionally workers).
  - Returns success so the UI can show "Waking... check back in a minute" or poll until the control plane is reachable.

**Configuration:**

- **CLI:** **`WAKE_URL`** and/or **`WAKE_SCRIPT`** from environment (`WAKE_URL`, `WAKE_SCRIPT`) or config file (`~/.config/remote-harness/config.yaml`: `wake_url`, `wake_script`). Precedence: env over config file. If both are set, **WAKE_URL wins** -- only the URL is invoked (HTTP GET). If only WAKE_SCRIPT is set, the CLI runs that script **on the machine where the CLI is running** (local path; CLI process spawns it). No remote execution. If unset, no "Wake up" action is shown.
- **Web UI:** In **Settings**, you can set a **Wake URL** (stored in the browser's localStorage). When the control plane is unreachable, the UI shows "Service unavailable. It may be sleeping." and an optional **Wake up** button that sends an HTTP GET to that URL. The browser cannot run a local script; **WAKE_SCRIPT** is only supported in the CLI.
- This keeps the harness agnostic to Tailscale, WOL, or any specific tool; you plug in whatever you use.

**Optional: wake portal on the always-on host**

- You can serve a minimal static page or app on the always-on host (e.g. "Remote Harness is sleeping" + "Wake up" button that calls your WOL sender, then redirects to the control plane UI once it's reachable). That portal is **your** deployment artifact, not part of the harness core. The harness only needs to support the configurable wake URL/script used by the main UI or that portal.

---

## 4a. Idle sleep / sleep-inhibit

The backend **does not** put the machine to sleep; it only reports whether there is work to coordinate. So the OS (or a host-side helper) can decide when to allow idle-sleep.

- **Live path:** **`GET /health/idle`** (no authentication). Load balancers and sleep helpers can call it without an API key.
- **200 OK** — No pending or assigned jobs; **OK for the OS to idle-sleep** (per your policy).
- **503 Service Unavailable** — There is work in flight; **hold sleep inhibit** (do not sleep).
- **Docker:** The inhibit lock must be taken on the **host**. Poll **from the host** (e.g. `curl http://localhost:3000/health/idle` on the published port), not only from inside the container. A small host-side daemon or systemd service can poll this endpoint and hold/release the OS sleep-inhibit API (e.g. IOPMAssertion on macOS, systemd-inhibit on Linux). See [GETTING_STARTED.md §1](GETTING_STARTED.md#1-docker-compose-recommended) for port and compose layout.

---

## 4b. UI hosting: public URL, client-only API access

When the control plane is reachable only on Tailscale (or your private network), the **Web UI can still be hosted publicly** (e.g. on your always-on host, a CDN, or any public URL). Access to the *application* remains restricted: only clients that can reach the control plane (e.g. browsers on devices that are on Tailscale) can actually use the app, because the browser must call the control plane **REST API** and **SSE** endpoints directly (v1: **SSE** for log tail and session events—no WebSocket for those streams).

**Implication for the UI stack:** All communication with the control plane must happen **from the browser**. The UI must not use **server-side code** in the chosen framework to proxy requests to the control plane or to fetch API data on the server (e.g. Next.js server components or Nuxt server routes that call the control plane). If it did, that UI server would need to be on Tailscale (or the same network) to reach the control plane, which would defeat hosting the UI on a separate "public" host. So:

- The Web UI should be built as a **client-side application** (SPA): the browser loads the app, then the browser opens **REST** and **SSE** connections (`EventSource` or fetch-based SSE) to the control plane URL (which may be a Tailscale URL when the user is remote).
- The UI framework should be used in a **client-only** way for control plane communication: client-side data fetching (e.g. TanStack Query in the browser), client-side SSE consumption, no server-side proxy or server-rendered API calls to the control plane. Static export or a build that serves only static assets plus client-side JS is a good fit.
- This is noted so that framework choice (e.g. Vite + React, not Next.js with server components that hit the API) and deployment stay consistent with "UI hosted anywhere, control plane only on Tailscale."

**CORS:** If the UI origin differs from the API origin (including port), the server must list the UI in **`CORS_ALLOWED_ORIGINS`**. See [§13 Production and first-run checklist](#13-production-and-first-run-checklist) and [TROUBLESHOOTING.md §1a](TROUBLESHOOTING.md#1a-cors-errors-in-the-browser).

---

## 5. Platform and environment support

To support control plane and workers on Windows (including WSL), macOS, and Linux:

- **Control plane:** Build and run on Windows (native or WSL), macOS, Linux. PostgreSQL or SQLite (for minimal/local) on the same machine or a reachable host.
- **Workers:** Same platforms; worker binary for each (e.g. `remote-harness-worker` for Windows, macOS, Linux). On Windows, WSL is optional (native Windows or WSL both valid).
- **CLI:** Same platforms; user runs it from any machine that can reach the control plane URL (e.g. via Tailscale when away).
- **Web UI:** Served by the control plane (or a reverse proxy). When the control plane is asleep, the UI is unreachable unless you use a separate wake portal (above) on the always-on host.

This gives you the flexibility to run the backend on your Windows PC, workers on the PC and Mac laptop, and use an always-on host only for wake (and optionally a small wake portal), without forcing that model on other deployments.

---

## 6. Recommended runtime: Docker Compose

For **consumer devices** (developer laptops, desktops) that may sleep, **Docker Compose** is the recommended way to run the backend, workers, and Postgres.

### 6.1 Why Docker Compose

| Property | Docker Compose on consumer device |
|----------|-----------------------------------|
| **Crash restart** | `restart: unless-stopped` -- container auto-restarts after panic/OOM while the host is awake. Already configured in our `docker-compose.yml`. |
| **Host sleep** | Docker Desktop (macOS/Windows) runs inside a lightweight VM (Apple Virtualization on macOS, WSL2 on Windows). **When the host sleeps, the VM suspends.** All containers freeze. On wake, the VM resumes, containers resume, the worker re-registers and heartbeats normally. Docker **does not prevent or fight OS sleep**. |
| **Wake recovery** | After wake: Docker daemon resumes -> container resumes (or restarts if it had crashed before sleep) -> worker heartbeats/polls -> picks up reclaimed jobs. Seamless, no manual "start the worker" step. |
| **Resource overhead** | Docker Desktop VM: ~1-2 GB idle RAM. Acceptable on a 16+ GB workstation. No K8s control plane, no etcd, no scheduler overhead. |
| **Multi-service** | Compose already manages `postgres`, `server`, `worker`, `web` in one file. Adding more workers = `docker compose up --scale worker=2`. |
| **Cross-platform** | Docker Desktop runs on macOS (Intel + Apple Silicon), Windows (WSL2), Linux (native). Same compose file everywhere. |
| **User stop** | `docker compose stop` stops everything; `unless-stopped` respects that (won't auto-restart until `docker compose up` again). |

### 6.2 Why not Kubernetes on consumer devices

K8s (via minikube, Docker Desktop K8s, Rancher Desktop, k3s) *technically* works and provides `restartPolicy: Always`:

| Property | K8s on consumer device |
|----------|------------------------|
| **Crash restart** | Yes -- kubelet restarts crashed pods with exponential backoff (CrashLoopBackOff). |
| **Host sleep** | Host CAN still sleep (same underlying VM as Docker Desktop). K8s reconciliation loops (kubelet, scheduler, controller-manager) resume after wake. |
| **Wake recovery** | After wake, kubelet needs to re-sync state. This **can take 30-90 seconds** of churn (pod status checks, lease renewals, node heartbeats). On a laptop waking from long sleep, the node may briefly report NotReady before self-healing. Functional but noisier than plain Docker resume. |
| **Resource overhead** | **Significantly higher:** etcd, kube-apiserver, kube-scheduler, kube-controller-manager, kubelet, kube-proxy -- each consuming CPU and RAM even when idle. minikube/k3s: ~500 MB-1 GB on top of Docker. Full Docker Desktop K8s: more. On a general workstation this is wasted when there's no work. |
| **Complexity** | Helm charts or manifests instead of (or in addition to) compose. `kubectl` tooling. Overkill for "1 backend + 1-2 workers on my laptop." |
| **Sleep-inhibit interaction** | K8s has no concept of "host should sleep." Our `/health/idle` -> sleep-inhibit helper pattern works the same, but K8s background daemons may themselves prevent idle detection at the OS level (the VM is always "doing something"). This can **fight** the sleep model. |

**Verdict:** K8s adds overhead and complexity with no real benefit for the "sleeping workstation" use case. Reserve it for **dedicated/cloud worker pools** (always-on nodes) where its scheduling and scaling features actually help.

### 6.3 When to use what

| Deployment target | Recommended runtime | Why |
|-------------------|---------------------|-----|
| **Developer laptop / desktop (sleeps)** | **Docker Compose** | Lightweight, crash restart, sleep-compatible, already shipped |
| **Dedicated server / VPS (always-on)** | Docker Compose or **K8s** | K8s adds value at scale (multiple workers, autoscaling, node pools) |
| **Cloud / EKS / GKE** | **K8s** | Native platform; restart, scaling, monitoring built in |
| **CI runner (ephemeral)** | Bare process or Docker | Short-lived; restart policy irrelevant |
| **Dev "just run it"** | `cargo run` in terminal | Acceptable for development; no crash restart |

---

## 7. Worker deployment shapes

| Type | Typical host | Process model | Sleep reality |
|------|----------------|---------------|---------------|
| **A. Docker worker** | Developer laptop, workstation, dev server | Container `worker` service via Compose | Host may sleep (laptop) or never (server). Container suspends/resumes with host. Restarts on crash while awake. **Recommended for most users.** |
| **B. Bare-metal / systemd / launchd worker** | Developer machine without Docker | `cargo run -p worker`, systemd unit, or launchd plist | User/OS puts machine to sleep -> process frozen; heartbeats stop -> stale; jobs reclaimed. Restart depends on supervisor (systemd/launchd) or manual. |
| **C. K8s worker** | Dedicated server, cloud VM (always-on) | Pod in Deployment | Restart policy is platform-native; host rarely sleeps. Good for scale; overkill for laptops. |
| **D. Ephemeral CI worker** | GitHub Actions, short-lived VM | One-shot or short loop | Less about restart; more about registration TTL and not leaving jobs assigned after job ends. |

---

## 8. Restart policy by platform

### 8.1 Docker Compose (recommended -- all platforms)

- **`restart: unless-stopped`** (already in compose): crash -> auto-restart and re-register. Sleep -> container suspends with host; resumes on wake. No manual intervention for either case.
- **Do not** set restart to `always` in a way that fights user stop; `unless-stopped` respects `docker compose stop`.
- **Docker Desktop resource settings:** Users can tune VM memory/CPU in Docker Desktop preferences. For our workload (postgres + server + worker), 2 CPU / 4 GB is comfortable.

### 8.2 systemd (bare-metal Linux)

- **Unit:** `Restart=on-failure` or `Restart=always` with **`RestartSec=5`** and **`StartLimitBurst=5`** to avoid tight crash loops.
- **Sleep:** systemd does not restart frozen processes during suspend. On resume, process continues (or if it had crashed, systemd restarts it).
- **Optional:** `User=` session unit so worker stops at logout if desired.

### 8.3 macOS launchd (bare-metal macOS)

- **Terminal session:** No restart -- one crash = dead until user restarts. Acceptable for dev only.
- **launchd plist with `KeepAlive`:** Restarts process on crash. Sleep: process suspends with OS; resumes on wake. Same stale behavior as Linux.
- **Recommendation:** Prefer Docker Compose over launchd for consistency with other platforms.

### 8.4 Windows (bare-metal)

- **Service with failure recovery:** Restart on failure (with delay). Sleep/hibernate suspends service; reclaim handles mid-task loss.
- **Recommendation:** Docker Desktop (WSL2 backend) + Compose is simpler and consistent. Native Windows service only if Docker is not an option.

### 8.5 Kubernetes (dedicated / cloud only)

- **`restartPolicy: Always`** on Deployment: pod restarts on crash. Reserve for always-on worker pools where K8s scheduling adds value.

---

## 9. Docker and sleep: detailed behavior

This section documents **exactly** what happens so we can be confident Docker Compose preserves our sleep model.

### 9.1 macOS (Docker Desktop with Apple Virtualization / HyperKit)

1. **User triggers sleep** (lid close, idle timer, manual): macOS sends sleep notification to all processes.
2. **Docker Desktop VM suspends:** The lightweight Linux VM that runs the Docker daemon freezes. All containers inside it freeze. No graceful SIGTERM to containers -- they just stop executing (like `SIGSTOP`).
3. **Host sleeps.**
4. **Host wakes:** VM resumes. Docker daemon resumes. Containers resume from where they left off (their process state was frozen in the VM's memory image).
5. **Worker process resumes:** It was in the middle of `tokio::time::interval.tick().await` (or a heartbeat HTTP call). The next tick fires; worker heartbeats the control plane. If enough time passed, the **old** worker id may be stale -- the worker's existing 404 -> re-register logic handles that.
6. **No manual intervention needed.**

### 9.2 Windows (Docker Desktop with WSL2)

Same model: WSL2 VM suspends when Windows sleeps/hibernates. On wake, VM resumes, containers resume. Identical to macOS from the container's perspective.

### 9.3 Linux (native Docker Engine)

No VM layer. Containers are just cgroups/namespaces on the host kernel. On suspend, all processes (including containerized ones) freeze. On resume, they continue. `restart: unless-stopped` handles any that crashed before suspend.

### 9.4 Does Docker Desktop prevent sleep?

**No.** Docker Desktop does not hold a sleep-inhibit lock. The VM is not "busy" from the OS's perspective -- it's just a process. macOS / Windows will put the machine to sleep according to normal power settings. Our **`/health/idle` -> sleep-inhibit** pattern works orthogonally: the **host-side helper** decides whether to inhibit sleep based on the backend's workload, not Docker's existence.

### 9.5 Docker Desktop "Resource Saver" mode

Docker Desktop 4.x+ has a "Resource Saver" feature that stops the VM after a period of container inactivity. This is **fine** for us: if all containers are idle (no active tasks), the VM can be paused by Docker Desktop. When a new request comes in or a container needs attention, the VM restarts. This actually **complements** our idle model -- idle harness = VM can be paused = less battery. On resume, containers restart (per `unless-stopped`), worker re-registers, and we're back.

---

## 10. Fit with host sleeping (architectural alignment)

| Concern | Docker Compose behavior |
|---------|-------------------------------|
| **Machine sleeps** | Containers freeze with VM/host; resume on wake. **Does not prevent sleep.** Stale worker + job reclaim handles mid-task loss. |
| **Process crash while awake** | `restart: unless-stopped` auto-restarts container; worker re-registers; reclaimed jobs picked up. |
| **Backend `/health/idle`** | Applies to **backend host** sleep-inhibit, not Docker itself. Host-side helper polls the endpoint and holds/releases OS inhibit lock. Docker is transparent. |
| **Wake-on-LAN / wake URL** | Unchanged: wakes the **physical machine**; Docker Desktop + containers come up with the host; worker auto-registers. |
| **Worker and backend on different hosts** | After wake, each process runs when its host is up. If the **control plane** is still sleeping, workers cannot register or pull until it is reachable again—this is ordinary connectivity, not Docker-specific. **Stale workers** and job reclaim use server thresholds such as **`worker_stale_seconds`** (often on the order of ~90s without heartbeat)—see [Architecture §3b](ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries). |

**Summary:** Docker Compose is **transparent** to the sleep model. It adds crash recovery without interfering with host power management. K8s adds overhead and potential interference (background daemons, reconciliation churn) that don't justify themselves on consumer devices.

---

## 11. Anti-patterns

- **K8s on a laptop** just for restart policy -- use Docker Compose instead.
- Expecting **restart alone** to fix jobs stuck on a **sleeping** machine -- use stale + reclaim (see [Architecture](ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries)).
- **Infinite restart** on a **buggy** worker without fixing the binary -- add backoff (Docker Compose has built-in restart backoff); fix panics per the panic hardening plan.
- Running **worker inside backend container** for production -- couples lifecycle; use separate services.
- **Docker Desktop always-on VM** fighting sleep -- not a real issue (VM suspends with host), but if users see Docker Desktop's VM consuming CPU on wake, that's normal reconciliation; it settles quickly.

---

## 12. Summary

| Aspect | Your (power-saving) setup | Others |
|--------|---------------------------|--------|
| Control plane + DB | On one of your machines (Windows or Mac); can sleep | Can be always-on server, cloud, or same as you |
| Workers | Windows PC (possibly WSL), Mac laptop; can sleep | Any reachable hosts |
| Runtime | **Docker Compose** (crash restart + sleep-compatible) | Docker Compose, K8s (cloud/dedicated), or bare-metal |
| Wake-up | Optional: wake URL/script -> your always-on host -> WOL | Not needed if always on |
| Connectivity | Tailscale (your choice) for remote access | Any network/VPN they prefer |
| UI when backend asleep | Unreachable unless you run a wake portal on always-on host | N/A or same |

The product remains topology-agnostic: it works with always-on deployments, sleepable deployments, and mixed setups. The only addition is an **optional, configurable wake integration** so that deployments like yours can offer a "Wake up" flow without the harness depending on WOL, Tailscale, or a specific always-on host.

---

## 13. Production and first-run checklist

Use this before exposing a deployment to others or to the public internet. It removes the most common “works on my laptop” gaps.

| Check | Why |
|--------|-----|
| **TLS** | Terminate HTTPS at a reverse proxy or the server; browsers and tunneled setups expect consistent `https://` for OAuth redirects. |
| **`CORS_ALLOWED_ORIGINS`** | Must include **every UI origin** (scheme + host + port) that will call the API. One missing origin → browser blocks all API calls from that UI. |
| **OAuth callback URLs** | `GITHUB_REDIRECT_URI` / `GITLAB_REDIRECT_URI` must match the OAuth app and the **control plane** public URL (API host), not the UI dev server port. |
| **`REDIRECT_AFTER_AUTH`** | After Git OAuth, redirect to a real UI URL (e.g. `https://your-ui/settings`) that users can reach. |
| **`POST /api-keys/bootstrap`** | **Dangerous** if the control plane is reachable by untrusted networks before the first key exists. Prefer: bind to localhost or VPN-only first, create a key, then widen access—or block bootstrap via firewall until configured. See [API_OVERVIEW §4c](API_OVERVIEW.md#4c-rest--api-keys-control-plane-auth). **Web UI:** show bootstrap **only** after a deliberate missing-key probe—not on every load ([CLIENT_EXPERIENCE §7](CLIENT_EXPERIENCE.md#7-first-time-setup-web-ui)). |
| **Homogeneous workers** | One pool per OS + installed agent CLI; avoid mixing incompatible workers on one control plane ([Architecture §4c](ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation)). |
| **Wake URL (optional)** | If the backend sleeps, configure [wake URL / CLI](HOSTING.md#4-wake-integration-optional-deployer-provided) so the UI can guide users instead of a dead page. |

**User-visible behavior** (errors, SSE reconnect, credentials): implementers should follow [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md).

---

## 14. Web UI threat model (API key in browser)

v1 stores the **control plane API key** in the browser (see [TECH_STACK.md §4](TECH_STACK.md)). **Implications for “rock solid” deployments:**

| Risk | Mitigation (operator) |
|------|------------------------|
| **XSS** steals API key | **Strict CSP**, no untrusted script tags, review dependencies; prefer same-origin UI hosting or known-good CDN. |
| **Shared device** | Treat the browser profile as **high trust**; issue **per-user API keys** and rotate when someone leaves. |
| **Shoulder surfing / devtools** | Same as any secret in localStorage—train users; **CLI** may be preferable for high-sensitivity environments. |
| **Bootstrap window** | While no key exists, **`POST /api-keys/bootstrap`** is equivalent to root—**never** expose the control plane port to the public internet until a key exists. Align UI with [CLIENT_EXPERIENCE §7](CLIENT_EXPERIENCE.md#7-first-time-setup-web-ui). |

End-user copy for safe use: [CLIENT_EXPERIENCE.md §11](CLIENT_EXPERIENCE.md#11-web-ui-and-api-key-operator-expectations).

---

*See also: [Architecture](ARCHITECTURE.md) (logical topology, worker death/reclaim), [Tech Stack](TECH_STACK.md) (platform support), [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md), [TROUBLESHOOTING.md](TROUBLESHOOTING.md).*
