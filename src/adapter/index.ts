import { tauriHostAdapter } from "./tauri";
import { webHostAdapter, HostAdapterError } from "./web";

export interface BrowserDragDropHandlers {
  onDragOver?: (e: DragEvent) => void;
  onDragLeave?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
}

export interface DragDropPayload {
  paths: string[];
  type: string;
  position: { x: number; y: number };
}

export interface HostAdapter {
  invoke<T>(cmd: string, args?: unknown): Promise<T>;
  listen(event: string, handler: (e: { payload: unknown }) => void): Promise<() => void>;
  getCurrentWebview(): {
    onDragDropEvent: (cb: (e: { payload: DragDropPayload }) => void) => Promise<() => void>;
  };
  isNative: boolean;
  isTauri: () => boolean;
}

function pick(): HostAdapter {
  if (typeof window !== "undefined" && (window as unknown as { __PAIGE_WEB__?: boolean }).__PAIGE_WEB__) {
    return webHostAdapter;
  }
  if ((window as unknown as { __TAURI__?: unknown }).__TAURI__) {
    return tauriHostAdapter;
  }
  if ((window as unknown as { __PAIGE_NATIVE__?: boolean }).__PAIGE_NATIVE__) {
    return tauriHostAdapter;
  }
  return webHostAdapter;
}

export const hostAdapter: HostAdapter = pick();

export { HostAdapterError };
export { tauriHostAdapter, webHostAdapter };
// BrowserDragDropHandlers and DragDropPayload already exported at declaration