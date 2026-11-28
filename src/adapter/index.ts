import { tauriHostAdapter } from "./tauri";
import { webHostAdapter, HostAdapterError } from "./web";

export interface HostAdapter {
  invoke<T>(cmd: string, args?: unknown): Promise<T>;
  listen(event: string, handler: (e: { payload: unknown }) => void): Promise<() => void>;
  isNative: boolean;
  isTauri: () => boolean;
}

function pick(): HostAdapter {
  // In a Tauri webview the native adapter is authoritative; in a browser
  // (hosted web / PAIGE Cloud) the cloud adapter is authoritative.
  if (typeof window !== "undefined" && (window as unknown as { __PAIGE_WEB__?: boolean }).__PAIGE_WEB__) {
    return webHostAdapter;
  }
  if ((window as unknown as { __TAURI__?: unknown }).__TAURI__) {
    return tauriHostAdapter;
  }
  // Browser: default to the cloud adapter unless explicitly native-flagged.
  if ((window as unknown as { __PAIGE_NATIVE__?: boolean }).__PAIGE_NATIVE__) {
    return tauriHostAdapter;
  }
  return webHostAdapter;
}

export const hostAdapter: HostAdapter = pick();

export { HostAdapterError };
export { tauriHostAdapter, webHostAdapter };
