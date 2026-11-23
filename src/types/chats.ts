// Frontend types for durable Direct Chats and execution evidence.

export interface ChatToolEvent {
  tool: string;
  ok: boolean;
  summary: string;
  timestamp: number;
}

export interface ChatMessage {
  role: string; // "user" | "assistant"
  text: string;
  tool_events?: ChatToolEvent[];
  model_label?: string;
  model?: string;
  gateway?: string;
  turn_id?: string;
}

export interface ExecutionEvent {
  timestamp: number;
  kind: string; // "model_turn" | "tool"
  tool: string;
  gateway: string;
  model: string;
  summary: string;
  outcome: string;
  details: string;
  actor: string; // "human" | "agent" | "unknown"
  turn_id?: string;
}

export type WorkflowTemplate = "engineering_v1" | "visual_qa_v1" | "research_v1";

export interface WorkflowRequest {
  template: WorkflowTemplate;
  workspace_root: string;
  restart_before_tests?: boolean;
  file_scope?: string;
  word_report?: boolean;
  selected_from_chat?: boolean;
  visual?: { before: string; css: string; proposal_only?: boolean; capture_window?: boolean };
}

export interface WorkflowRun extends WorkflowRequest {
  turn_id: string;
  requested_at: number;
}

export interface Chat {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  archived: boolean;
  team?: ChatTeam;
  specialists?: TeamMember[];
  project_roots: string[];
  no_project?: boolean;
  messages: ChatMessage[];
  execution: ExecutionEvent[];
  recorded_turn_ids: string[];
  workflow_runs?: WorkflowRun[];
  resume?: {
    turn_id: string;
    rounds: number;
    root: string | null;
    blocked: string | null;
    waiting_for_workspace?: boolean;
    restart_ready?: boolean;
    restart_waiting?: boolean;
    can_retry_openrouter_free?: boolean;
    read: boolean;
    write: boolean;
  };
}

export interface TeamMember {
  slug: string;
  name: string;
  description?: string | null;
}

export interface ChatTeam {
  connection_id: string;
  members: TeamMember[];
}

export interface ChatListResult {
  chats: Chat[];
  active_id: string | null;
}
