import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { SettingsProvider } from "../context/SettingsProvider";
import { SessionCreatePage } from "./SessionCreatePage";

describe("SessionCreatePage", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("rh_control_plane_url", "http://127.0.0.1:3000");
    localStorage.setItem("rh_api_key", "test-key");
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const req = input instanceof Request ? input : new Request(input, init);
        const path = new URL(req.url).pathname;
        const method = req.method;
        if (path.startsWith("/identities/") && method === "GET") {
          return new Response(JSON.stringify({ has_git_token: true, has_agent_token: true }), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });
        }
        if (path === "/workers" && method === "GET") {
          return new Response(
            JSON.stringify({
              items: [{ worker_id: "w1", labels: { platform: "linux" }, status: "active" }],
              next_cursor: null,
            }),
            { status: 200, headers: { "Content-Type": "application/json" } },
          );
        }
        if (path === "/sessions" && method === "POST") {
          return new Response(
            JSON.stringify({ session_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", status: "pending" }),
            { status: 201, headers: { "Content-Type": "application/json" } },
          );
        }
        return new Response("not found", { status: 404 });
      }),
    );
    vi.stubGlobal("confirm", vi.fn(() => true));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("submits create and navigates to session detail", async () => {
    const user = userEvent.setup();
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <SettingsProvider>
          <MemoryRouter initialEntries={["/sessions/new"]}>
            <Routes>
              <Route path="/sessions/new" element={<SessionCreatePage />} />
              <Route path="/sessions/:sessionId" element={<div data-testid="session-detail">detail</div>} />
            </Routes>
          </MemoryRouter>
        </SettingsProvider>
      </QueryClientProvider>,
    );

    await user.type(screen.getByLabelText(/Repository URL/i), "https://github.com/o/r.git");
    await user.type(screen.getByLabelText(/^Prompt/i), "Hello");

    await user.click(screen.getByRole("button", { name: /Start session/i }));

    await waitFor(() => {
      expect(screen.getByTestId("session-detail")).toBeInTheDocument();
    });
  });
});
