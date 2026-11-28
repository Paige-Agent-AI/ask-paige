import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview as tauriGetCurrentWebview } from "@tauri-apps/api/webview";
import { DragDropPayload, HostAdapter } from "./index";

export const tauriHostAdapter: HostAdapter = {
  async invoke<T>(cmd: string, args?: unknown): Promise<T> {
    return tauriInvoke(cmd, args as never) as Promise<T>;
  },

  async listen(event: string, handler: (e: { payload: unknown }) => void): Promise<UnlistenFn> {
    return tauriListen(event, handler);
  },

  getCurrentWebview() {
    const webview = tauriGetCurrentWebview();
    return {
      onDragDropEvent: (cb: (e: { payload: DragDropPayload }) => void) =>
        webview
          .onDragDropEvent((e: unknown) =>
            cb({ payload: e as unknown as DragDropPayload }),
          )
          .then((unlisten: () => void) => () => unlisten()),
    };
  },

  isNative: true,
  isTauri: () => isTauri(),
};