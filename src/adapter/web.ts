/**
 * Web/Cloud implementation of the PAIGE host adapter.
 *
 * This adapter targets PAIGE Cloud HTTP endpoints. Commands not yet
 * implemented against Cloud throw `HostAdapterError` with code
 * "UNAVAILABLE" — callers must handle gracefully (e.g., hide UI,
 * show "desktop only" notice, or disable).
 *
 * When a Cloud endpoint exists, replace the throw with a `fetch` call.
 * Endpoint convention: POST /api/host/{command} with JSON body = args.
 */
export class HostAdapterError extends Error {
  constructor(
    public readonly code: "UNAVAILABLE" | "UNAUTHORIZED" | "FAILED",
    message: string,
    public readonly command?: string,
  ) {
    super(message);
    this.name = "HostAdapterError";
  }
}

const CLOUD_BASE = (import.meta.env.VITE_PAIPE_CLOUD_API ?? "").replace(/\/+$/, "");

async function cloudFetch<T>(command: string, args?: unknown): Promise<T> {
  if (!CLOUD_BASE) {
    throw new HostAdapterError("UNAVAILABLE", "PAIGE Cloud API base not configured (VITE_PAIPE_CLOUD_API)");
  }
  const res = await fetch(`${CLOUD_BASE}/api/host/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify(args ?? {}),
  });
  if (res.status === 401) throw new HostAdapterError("UNAUTHORIZED", "Not authenticated with PAIGE Cloud");
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new HostAdapterError("FAILED", `Cloud ${command} failed: ${res.status} ${text}`);
  }
  return res.json() as Promise<T>;
}

/** Web/Cloud implementation of the PAIGE host adapter. */
export const webHostAdapter = {
  async invoke<T>(cmd: string, args?: unknown): Promise<T> {
    // Model Fabric routing — these are the only commands with qualified Cloud endpoints
    const cloudCommands = new Set([
      "ask_connect_openrouter",
      "ask_disconnect_openrouter",
      "ask_connect_omniroute",
      "ask_disconnect_omniroute",
      "ask_set_model_selection",
      "ask_status",
      "ask_cancel",
    ]);

    if (cloudCommands.has(cmd)) {
      return cloudFetch<T>(cmd, args);
    }

    // Native-only commands — explicitly unavailable on web
    throw new HostAdapterError(
      "UNAVAILABLE",
      `Command "${cmd}" requires native desktop (Tauri). Use the Windows app for this feature.`,
      cmd,
    );
  },

  async listen(_event: string, _handler: (e: { payload: unknown }) => void) {
    // WebSocket / SSE not yet implemented for Cloud events
    return () => {};
  },

  isNative: false,
  isTauri: () => false,
};

/** Web drag-drop is handled by the file input; no Tauri webview events exist. */
export function getCurrentWebview(): { onDragDropEvent: (cb: (e: { payload: { paths: string[] } }) => void) => Promise<() => void> } {
  return {
    onDragDropEvent: (_cb: (e: { payload: { paths: string[] } }) => void) => Promise.resolve(() => {}),
  };
}