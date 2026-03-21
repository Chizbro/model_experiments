/**
 * Incremental SSE parser: feed UTF-8 text chunks; invoke callback per complete event (blank line).
 * Matches comment lines (`:`), `event:`, and `data:` framing used by the control plane.
 */
export class SseLineBuffer {
  private buf = "";
  private pendingEvent: string | undefined;
  private dataLines: string[] = [];

  push(chunk: string, onEvent: (eventName: string | undefined, data: string) => void): void {
    this.buf += chunk;
    for (;;) {
      const nl = this.buf.indexOf("\n");
      if (nl < 0) break;
      const rawLine = this.buf.slice(0, nl);
      this.buf = this.buf.slice(nl + 1);
      const line = rawLine.replace(/\r$/, "");
      if (line === "") {
        if (this.pendingEvent !== undefined || this.dataLines.length > 0) {
          onEvent(this.pendingEvent, this.dataLines.join("\n"));
          this.pendingEvent = undefined;
          this.dataLines = [];
        }
        continue;
      }
      if (line.startsWith(":")) {
        continue;
      }
      if (line.startsWith("event:")) {
        this.pendingEvent = line.slice(6).trim();
        continue;
      }
      if (line.startsWith("data:")) {
        const rest = line.slice(5);
        this.dataLines.push(rest.startsWith(" ") ? rest.slice(1) : rest);
      }
    }
  }
}
