export type BrokerStatus = {
  granted: boolean;
  root: string | null;
  read: boolean;
  write: boolean;
  pending_proposals: number;
  session_id: string;
  allowed_roots: string[];
  remembered_roots: string[];
  terminal_available: boolean;
};

export type FileEntry = {
  rel_path: string;
  size: number;
};

export type FileRead = {
  path: string;
  content: string;
  sha256: string;
  size: number;
};

export type ProposalInfo = {
  proposal_id: string;
  path: string;
  before_sha256: string;
  approval_token: string;
  expires_in_seconds: number;
  new_size: number;
};

export type WriteReceipt = {
  path: string;
  before_sha256: string;
  after_sha256: string;
  bytes_written: number;
  readback_verified: boolean;
  outcome: string;
};

export type OperationRecord = {
  timestamp: number;
  session_id: string;
  proposal_id: string;
  path: string;
  before_sha256: string;
  after_sha256: string;
  outcome: string;
};

export type ReconcileResult = {
  proposal_id: string;
  path: string;
  current_sha256: string;
  before_sha256: string;
  after_sha256: string;
  outcome: string;
};
