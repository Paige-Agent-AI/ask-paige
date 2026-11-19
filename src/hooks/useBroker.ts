import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  BrokerStatus,
  FileEntry,
  FileRead,
  OperationRecord,
  ProposalInfo,
  ReconcileResult,
  WriteReceipt,
} from "../types/broker";

const LAB_ROOT = "C:\\Users\\tonig\\paige-desktop-lab";

export type BrokerError = { message: string };

export function useBroker() {
  const [status, setStatus] = useState<BrokerStatus | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<FileRead | null>(null);
  const [proposals, setProposals] = useState<ProposalInfo[]>([]);
  const [operations, setOperations] = useState<OperationRecord[]>([]);
  const [busy, setBusy] = useState(false);
  const [lastError, setLastError] = useState<BrokerError | null>(null);
  const [lastNotice, setLastNotice] = useState<string | null>(null);

  const notify = useCallback((text: string) => {
    setLastNotice(text);
    setLastError(null);
  }, []);

  const fail = useCallback((text: string) => {
    setLastError({ message: text });
    setLastNotice(null);
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const s = (await invoke("broker_status")) as BrokerStatus;
      setStatus(s);
    } catch (e) {
      fail("Status refresh failed: " + String(e));
    }
  }, [fail]);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const grant = useCallback(async () => {
    setBusy(true);
    try {
      const s = (await invoke("broker_grant_root", {
        root: LAB_ROOT,
        read: true,
        write: true,
      })) as BrokerStatus;
      setStatus(s);
      notify("Granted read+write access to " + (s.root ?? LAB_ROOT));
    } catch (e) {
      fail("Grant failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [notify, fail]);

  const revoke = useCallback(async () => {
    setBusy(true);
    try {
      const s = (await invoke("broker_revoke")) as BrokerStatus;
      setStatus(s);
      setFiles([]);
      setSelectedFile(null);
      notify("Access revoked");
    } catch (e) {
      fail("Revoke failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [notify, fail]);

  const listFiles = useCallback(async () => {
    setBusy(true);
    try {
      const f = (await invoke("broker_list_files")) as FileEntry[];
      setFiles(f);
      notify("Listed " + f.length + " file" + (f.length === 1 ? "" : "s"));
    } catch (e) {
      fail("List files failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [notify, fail]);

  const readFile = useCallback(
    async (relPath: string) => {
      setBusy(true);
      try {
        const r = (await invoke("broker_read_file", { path: relPath })) as FileRead;
        setSelectedFile(r);
        notify("Read " + relPath);
      } catch (e) {
        fail("Read failed: " + String(e));
      } finally {
        setBusy(false);
      }
    },
    [notify, fail]
  );

  const proposeWrite = useCallback(
    async (relPath: string, newContent: string, expectedSha256: string) => {
      setBusy(true);
      try {
        const p = (await invoke("broker_propose_write", {
          path: relPath,
          newContent,
          expectedSha256,
        })) as ProposalInfo;
        setProposals((prev) => [p, ...prev]);
        notify("Proposal " + p.proposal_id + " created");
        return p;
      } catch (e) {
        fail("Propose failed: " + String(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [notify, fail]
  );

  const approve = useCallback(
    async (proposalId: string, token: string) => {
      setBusy(true);
      try {
        const r = await invoke("broker_approve", { proposalId, approvalToken: token });
        notify("Approval: " + String(r));
        return true;
      } catch (e) {
        fail("Approval failed: " + String(e));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [notify, fail]
  );

  const deny = useCallback(
    async (proposalId: string) => {
      setBusy(true);
      try {
        const r = await invoke("broker_deny", { proposalId });
        setProposals((prev) => prev.filter((p) => p.proposal_id !== proposalId));
        notify("Denied: " + String(r));
      } catch (e) {
        fail("Deny failed: " + String(e));
      } finally {
        setBusy(false);
      }
    },
    [notify, fail]
  );

  const applyWrite = useCallback(
    async (proposalId: string) => {
      setBusy(true);
      try {
        const r = (await invoke("broker_apply", { proposalId })) as WriteReceipt;
        setProposals((prev) => prev.filter((p) => p.proposal_id !== proposalId));
        setOperations((prev) => [
          {
            timestamp: Date.now() / 1000,
            session_id: status?.session_id ?? "unknown",
            proposal_id: proposalId,
            path: r.path,
            before_sha256: r.before_sha256,
            after_sha256: r.after_sha256,
            outcome: r.outcome,
          },
          ...prev,
        ]);
        notify(
          "Applied " +
            r.path +
            " — " +
            (r.readback_verified ? "verified" : "unverified") +
            " (" +
            r.outcome +
            ")"
        );
        return r;
      } catch (e) {
        fail("Apply failed: " + String(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [notify, fail, status?.session_id]
  );

  const loadOperations = useCallback(async () => {
    setBusy(true);
    try {
      const ops = (await invoke("broker_operations")) as OperationRecord[];
      setOperations(ops);
      notify("Loaded " + ops.length + " operation record" + (ops.length === 1 ? "" : "s"));
    } catch (e) {
      fail("Load activity failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [notify, fail]);

  const reconcile = useCallback(
    async (proposalId: string) => {
      setBusy(true);
      try {
        const rec = (await invoke("broker_reconcile", { proposalId })) as ReconcileResult;
        notify("Reconcile: " + rec.outcome + " for " + rec.path);
        return rec;
      } catch (e) {
        fail("Reconcile failed: " + String(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [notify, fail]
  );

  const clearFeedback = useCallback(() => {
    setLastError(null);
    setLastNotice(null);
  }, []);

  return {
    status,
    files,
    selectedFile,
    proposals,
    operations,
    busy,
    lastError,
    lastNotice,
    refreshStatus,
    grant,
    revoke,
    listFiles,
    readFile,
    proposeWrite,
    approve,
    deny,
    applyWrite,
    loadOperations,
    reconcile,
    clearFeedback,
    setSelectedFile,
  };
}
