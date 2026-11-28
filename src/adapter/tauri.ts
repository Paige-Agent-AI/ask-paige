import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

/** Tauri-native implementation of the PAIGE host adapter. */
export const tauriHostAdapter = {
  async invoke<T>(cmd: string, args?: unknown): Promise<T> {
    return tauriInvoke(cmd, args as never) as Promise<T>;
  },

  async listen(event: string, handler: (e: { payload: unknown }) => void): Promise<UnlistenFn> {
    return tauriListen(event, handler);
  },

  isNative: true,
  isTauri: () => isTauri(),
};