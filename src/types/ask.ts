export interface GatewayHealth {
  gateway: string;
  label: string;
  reachable: boolean;
  authenticated: boolean;
  catalog_count: number;
  detail: string;
  rank: number;
}

export interface ModelCandidate {
  id: string;
  gateway: string;
  coding_tier: number;
  tools: boolean;
  context_bytes: number;
}

export interface ModelSelection {
  selected: ModelCandidate | null;
  rationale: string;
  manual_pin_honored: boolean;
  considered: number;
}

export interface AskStatus {
  gateways: GatewayHealth[];
  catalog: ModelCandidate[];
  selection: string;
  manual_pin: string | null;
  openrouter_connected: boolean;
}

export interface AskReceipt {
  timestamp: number;
  turn_id: string;
  kind: string;
  tool: string;
  gateway: string;
  model: string;
  summary: string;
  outcome: string;
  details: string;
}

export interface ToolResultEvent {
  turn_id: string;
  tool: string;
  call_id: string;
  ok: boolean;
  summary: string;
  output: string;
}

export interface AskTurnStarted {
  turn_id: string;
  selection: ModelSelection;
}

export type ChatRole = "user" | "assistant" | "system";

export interface ChatToolEvent {
  tool: string;
  ok: boolean;
  summary: string;
  timestamp: number;
}
