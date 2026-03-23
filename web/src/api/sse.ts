import { getApiKey, getControlPlaneUrl } from './client';

/**
 * Fetch-based SSE client.
 *
 * Uses the Fetch API (not EventSource) so we can attach Authorization headers.
 * Parses the text/event-stream format manually and invokes callbacks per event.
 * Automatically reconnects with exponential backoff on disconnect.
 */

export interface SSECallbacks {
  onEvent: (eventType: string, data: string) => void;
  onError?: (error: Error) => void;
  onReconnecting?: () => void;
  onOpen?: () => void;
}

export interface SSEConnection {
  close: () => void;
}

const MAX_BACKOFF_MS = 30_000;
const INITIAL_BACKOFF_MS = 1_000;

export function connectSSE(path: string, callbacks: SSECallbacks): SSEConnection {
  let abortController = new AbortController();
  let closed = false;
  let backoff = INITIAL_BACKOFF_MS;

  async function connect() {
    if (closed) return;

    const base = getControlPlaneUrl();
    const key = getApiKey();

    if (!base || !key) {
      callbacks.onError?.(new Error('Control plane URL or API key not configured.'));
      return;
    }

    try {
      abortController = new AbortController();
      const res = await fetch(`${base}${path}`, {
        headers: {
          Authorization: `Bearer ${key}`,
          Accept: 'text/event-stream',
        },
        signal: abortController.signal,
      });

      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(`SSE connection failed: ${res.status} ${text}`);
      }

      if (!res.body) {
        throw new Error('SSE response has no body');
      }

      callbacks.onOpen?.();
      backoff = INITIAL_BACKOFF_MS;

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let currentEventType = 'message';
      let currentData = '';

      let reading = true;
      while (reading) {
        const { done, value } = await reader.read();
        if (done) { reading = false; break; }

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        // Keep the last potentially incomplete line in the buffer
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (line === '') {
            // Empty line = end of event
            if (currentData) {
              callbacks.onEvent(currentEventType, currentData.trimEnd());
              currentData = '';
              currentEventType = 'message';
            }
          } else if (line.startsWith('event:')) {
            currentEventType = line.slice(6).trim();
          } else if (line.startsWith('data:')) {
            const payload = line.slice(5);
            // SSE spec: if there's a space after "data:", strip it
            const trimmed = payload.startsWith(' ') ? payload.slice(1) : payload;
            currentData += trimmed + '\n';
          } else if (line.startsWith(':')) {
            // Comment, ignore
          } else if (line.startsWith('id:')) {
            // We don't use Last-Event-ID in this implementation
          } else if (line.startsWith('retry:')) {
            const retry = parseInt(line.slice(6).trim(), 10);
            if (!isNaN(retry)) {
              backoff = Math.min(retry, MAX_BACKOFF_MS);
            }
          }
        }
      }
    } catch (err) {
      if (closed) return;
      if (err instanceof DOMException && err.name === 'AbortError') {
        return;
      }
      callbacks.onError?.(err instanceof Error ? err : new Error(String(err)));
    }

    // Connection ended (stream closed or error). Reconnect unless manually closed.
    if (!closed) {
      callbacks.onReconnecting?.();
      await new Promise((r) => setTimeout(r, backoff));
      backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
      connect();
    }
  }

  connect();

  return {
    close() {
      closed = true;
      abortController.abort();
    },
  };
}
