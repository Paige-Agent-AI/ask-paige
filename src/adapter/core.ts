/**
 * Compatibility shim: mirrors `@tauri-apps/api/core` and `@tauri-apps/api/event`
 * signatures, delegating to `hostAdapter` on web and native Tauri on desktop.
 *
 * Callers keep using `invoke<T>(cmd, args)` and `listen<T>(event, handler)` with
 * the same type parameters they've always used — no code changes needed.
 */
import { isTauri as isTauriNative } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { hostAdapter, HostAdapterError } from "./index";

/** Generic invoke: callers use invoke<T>(cmd, args) as before. */
export function invoke<T>(cmd: string, args?: unknown): Promise<T> {
  return hostAdapter.invoke<T>(cmd, args);
}

/** Generic listen: returns UnlistenFn; callers use listen<T>(event, handler) as before. */
export function listen<T>(
  event: string,
  handler: (e: { payload: T }) => void,
): Promise<UnlistenFn> {
  if (hostAdapter.isNative) {
    return tauriListen(event, handler as (e: { payload: unknown }) => void);
  }
  return hostAdapter.listen(event, handler as (e: { payload: unknown }) => void) as Promise<UnlistenFn>;
}

export function isTauri(): boolean {
  return isTauriNative();
}

export { HostAdapterError };
export type { UnlistenFn };