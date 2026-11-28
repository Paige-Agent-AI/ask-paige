/**
 * Web/Cloud implementation of the PAIGE host adapter.
 *
 * This adapter targets PAIGE Cloud endpoints. All command invocation and
 * event subscription against PAIGE Cloud is currently UNAVAILABLE because
 * no qualified Cloud contract has been established by the original Cloud
 * custodian (ANT-169/176). Do not treat client-side adapter selection as
 * authorization, identity, or execution authority.
 *
 * Capability states are honest:
 *   - throw HostAdapterError("UNAVAILABLE") for unimplemented commands
 *   - throw HostAdapterError("UNAVAILABLE") for event subscriptions
 *
 * Browser file selection and browser drag-drop are implemented through
 * appropriate browser APIs (HTML input[type=file], DragEvent) behind
 * qualified content-access contracts — they are not routed to Tauri.
 */

import { HostAdapter, BrowserDragDropHandlers } from "./index";

/**
 * Capability state returned when a command is not qualified for Cloud.
 * Callers must inspect this before attempting any operation.
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

/**
 * Browser-native drag-drop handlers using standard DragEvent APIs.
 * No Tauri webview dependency. Subscribers handle paths as browser FileList
 * references or DataTransfer items, subject to qualified content-access.
 */
export interface BrowserDragDrop {
  onDragOver?: (e: DragEvent) => void;
  onDragLeave?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
}

/**
 * Register browser drag-drop handlers on a given element. Returns an
 * unsubscribe function. No Tauri webview events are involved.
 */
export function registerBrowserDragDrop(
  element: HTMLElement,
  handlers: BrowserDragDropHandlers,
): () => void {
  const onDragOver = (e: DragEvent) => { e.preventDefault(); handlers.onDragOver?.(e); };
  const onDragLeave = (e: DragEvent) => { e.preventDefault(); handlers.onDragLeave?.(e); };
  const onDrop = (e: DragEvent) => { e.preventDefault(); handlers.onDrop?.(e); };

  element.addEventListener("dragover", onDragOver);
  element.addEventListener("dragleave", onDragLeave);
  element.addEventListener("drop", onDrop);

  return () => {
    element.removeEventListener("dragover", onDragOver);
    element.removeEventListener("dragleave", onDragLeave);
    element.removeEventListener("drop", onDrop);
  };
}

/**
 * Web drag-drop: browser-native. Returns a handle compatible with the
 * Tauri path so Attachments.tsx compiles identically on web.
 * Actual subscription goes through registerBrowserDragDrop().
 */
export function getCurrentWebview() {
  return {
    onDragDropEvent: (_cb: (e: { payload: { paths: string[]; type: string; position: { x: number; y: number } } }) => void) => Promise.resolve(() => {}),
  };
}

/** Web/Cloud implementation of the PAIGE host adapter. */
export const webHostAdapter: HostAdapter = {
  async invoke<T>(_cmd: string, _args?: unknown): Promise<T> {
    // All Cloud command invocation is UNAVAILABLE until the original Cloud
    // custodian supplies actual versioned contracts (ANT-169/176).
    throw new HostAdapterError(
      "UNAVAILABLE",
      `Cloud command is not yet qualified against PAIGE Cloud. Coordinate actual versioned contracts through ANT-169/176 before use.`,
    );
  },

  /**
   * Event subscriptions are UNAVAILABLE on web until actual streaming,
   * progress, cancellation and durable execution-event contracts are
   * coordinated with the original Cloud and shared-system custodians.
   * Do not represent an inactive event channel as successfully connected.
   */
  async listen(_event: string, _handler: (e: { payload: unknown }) => void): Promise<() => void> {
    throw new HostAdapterError(
      "UNAVAILABLE",
      "Cloud event subscriptions are not yet qualified. Coordinate streaming/progress/cancellation contracts through ANT-169/176.",
    );
  },

  getCurrentWebview() {
    return getCurrentWebview();
  },

  isNative: false,
  isTauri: () => false,
};