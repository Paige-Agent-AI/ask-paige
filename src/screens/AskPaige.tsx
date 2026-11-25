import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Markdown from "react-markdown";
import { Composer } from "../ask/Composer";
import { Welcome } from "../ask/Welcome";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { ActivityIcon, ArrowUpIcon, ChevronLeftIcon, ChevronRightIcon, CpuIcon, FileTextIcon, FolderIcon, NewChatIcon, SettingsIcon } from "../components/Icons";
import type { Screen } from "../components/Layout";
import { PaigeCommandMark } from "../components/brand/PaigeCommandMark";
import { requestsVisualProposal, resolveChatIntent } from "../conversationIntent";
import { useBroker } from "../hooks/useBroker";
import { useAsk, type AskMode, type ComposerState } from "../hooks/useAsk";
import type { ExecutionEvent, Chat, ChatMessage, ChatListResult, WorkflowTemplate } from "../types/chats";
import { mergeWorkflowEvidence, VerifiedPng, VisualScreenshot, WorkflowDetails, workflowTemplates } from "./Workflows";
import { Files } from "./Files";
import { Terminal } from "./Terminal";
import { ApprovalCard, hasWritePreview, ModelRoundFacts, ProposedAction, readReceipt, SearchResults } from "./WorkbenchReview";
import { isWorkbenchPlan, WorkbenchPlan } from "./WorkbenchPlan";
import { SkillDiscovery, type SkillSearch } from "../components/SkillDiscovery";
import type { InstalledSkill, SkillInfo } from "../skills";
import { TeamPicker } from "./TeamPicker";
import { ModelPicker, modelLabel } from "./ModelPicker";
import type { BrokerStatus, FileEntry } from "../types/broker";
import { folderName, LAB_ROOT, rememberFolder, savedFolders } from "../projectFolders";
import { liveRunLabel, runStep } from "./runProgress";
import "./AskPaige.css";

const SLASH_COMMANDS: { cmd: string; label: string; prompt: string }[] = [
  { cmd: "/inspect", label: "Inspect the project", prompt: "Inspect the authorized project: list its files, read the main source files, and summarize the structure and purpose of each file." },
  { cmd: "/test", label: "Run the project tests", prompt: "Run the project's tests and report the exact output and pass/fail result." },
  { cmd: "/build", label: "Build the project", prompt: "Build the project and report the exact build output and result." },
  { cmd: "/review", label: "Review the code", prompt: "Review the project source for correctness issues and improvement opportunities. Cite file names and lines." },
  { cmd: "/explain", label: "Explain the code", prompt: "Explain what the project source does, file by file, in clear language." },
  { cmd: "/web", label: "Search the public web", prompt: "/web " },
  { cmd: "/show-image", label: "Show a project PNG in chat", prompt: "/show-image " },
  { cmd: "/screenshot", label: "Capture this PAIGE window", prompt: "/screenshot" },
  { cmd: "/create-image", label: "Create an image with OpenRouter", prompt: "/create-image " },
  { cmd: "/research", label: "Research a question", prompt: "Research this question using the project sources and give a sourced answer: " },
  { cmd: "/workspace", label: "Check workspace access", prompt: "/workspace" },
  { cmd: "/linear", label: "Look up a Linear issue", prompt: "/linear " },
  { cmd: "/notion", label: "Read the selected Notion page", prompt: "/notion" },
  { cmd: "/github", label: "Look up a GitHub issue", prompt: "/github #" },
  { cmd: "/supabase-logs", label: "Check Supabase logs", prompt: "/supabase-logs postgres 15" },
];

const workbenchTabs = ["Plan", "Changes", "Files", "Terminal", "Activity"] as const;
type WorkbenchTab = typeof workbenchTabs[number];
type ComposerMenu = "options" | "skills" | "workflows";
type ChatDraft = {
  input: string;
  mode: AskMode;
  skillId: string | null;
  attachments: string[];
  contextRefs: string[];
  workflowDraft: WorkflowTemplate | null;
  workflowIntentText: string | null;
  workflowChoices: WorkflowTemplate[];
  skipIntentText: string | null;
  visualProposalOnly: boolean;
  restartBeforeTests: boolean;
  workflowFile: string;
  beforePng: string;
  captureWindow: boolean;
};

const chatModelPreference = "paige.chatModelOverrides";
function readChatModels(): Record<string, string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(chatModelPreference) ?? "null");
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return Object.fromEntries(Object.entries(parsed).filter(([id, model]) => id && typeof model === "string"));
    }
  } catch { /* Fall back to the configured model. */ }
  return {};
}

function fmtTs(ms: number): string {
  const d = new Date(ms < 100_000_000_000 ? ms * 1000 : ms);
  return `${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })} · ${d.toLocaleDateString([], { month: "short", day: "2-digit", year: "numeric" })}`;
}

function receiptElapsed(event: ExecutionEvent) {
  const { elapsed_ms: elapsed } = readReceipt(event);
  if (typeof elapsed !== "number" || !Number.isFinite(elapsed) || elapsed < 0) return undefined;
  if (elapsed === 0) return "<1 ms";
  return elapsed < 1000 ? `${elapsed} ms` : `${(elapsed / 1000).toFixed(2)} s`;
}

const connectorReceiptLabels: Record<string, string> = {
  "linear-mcp": "Linear MCP receipt",
  "notion-mcp": "Notion MCP receipt",
  "github-mcp": "GitHub MCP receipt",
  "supabase-mcp": "Supabase log receipt",
  "omniroute-web": "Web search receipt",
  "skills-discovery": "Skill search receipt",
};

function Receipt({ event }: { event: ExecutionEvent }) {
  const elapsed = receiptElapsed(event);
  let details = event.details;
  let previews: { path: string; content: string }[] = [];
  try {
    const result = JSON.parse(details);
    if (typeof result?.path === "string" && typeof result?.content === "string") {
      previews = [{ path: result.path, content: result.content }];
    } else if (Array.isArray(result?.diffs)) {
      previews = (result.diffs as { path: string; patch: string }[])
        .filter((diff) => typeof diff?.path === "string" && typeof diff?.patch === "string")
        .map((diff) => ({ path: diff.path, content: diff.patch }));
    }
    details = JSON.stringify(result, null, 2);
  } catch {
    // Older receipts can contain plain text or partial JSON. Show it as saved.
  }
  return (
    <article className="ask-artifact">
      <div className="ask-artifact-meta">
        <strong>{event.tool}</strong>
        <span>{event.outcome.replace(/_/g, " ")}</span>
        <span>{event.actor.startsWith("@") ? event.actor : event.actor === "agent" ? "PAIGE" : "Identity unverified"}</span>
      </div>
      <div className="ask-artifact-summary">{event.summary}</div>
      <div className="ask-artifact-meta">
        <time>{fmtTs(event.timestamp)}</time>
        {elapsed && <span>Elapsed {elapsed}</span>}
        {event.gateway && <span>{[event.model, event.gateway].filter(Boolean).join(" · ")}</span>}
      </div>
      {event.tool === "model_round" && <ModelRoundFacts event={event} />}
      {event.tool === "web_search" && <SearchResults event={event} />}
      {previews.map(({ path, content }, index) => (
        <details key={index}>
          <summary>{path}</summary>
          <pre className="ask-artifact-details">{content}</pre>
        </details>
      ))}
      {details && (
        <details>
          <summary>Full receipt</summary>
          <pre className="ask-artifact-details">{details}</pre>
        </details>
      )}
    </article>
  );
}

interface ProjectEntry { id: string; name: string; root: string; granted_at: number; revoked: boolean; }

const rootKeyOf = (root: string) => root.replace(/\//g, "\\").replace(/[\\]+$/, "").toLowerCase();

function turnRanNoTools(events: ExecutionEvent[]): boolean {
  const final = events.find((event) => event.tool === "final_answer" && event.outcome === "complete");
  if (!final) return false;
  try {
    const details = JSON.parse(final.details) as { rounds?: number };
    return details.rounds === 0;
  } catch {
    return false;
  }
}

function ReplyActions({ text, onActivity }: { text: string; onActivity?: (trigger: HTMLButtonElement | null) => void }) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const firstAction = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [feedback, setFeedback] = useState("");

  useEffect(() => { if (open) firstAction.current?.focus(); }, [open]);

  const close = () => {
    setOpen(false);
    button.current?.focus({ preventScroll: true });
  };

  return (
    <div className="ask-reply-actions"
      onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false); }}
      onKeyDown={(event) => {
        if (open && event.key === "Escape") {
          event.stopPropagation();
          close();
        }
      }}>
      <button type="button" className="ask-reply-copy" onClick={async () => {
        try { await navigator.clipboard.writeText(text); setFeedback("Copied reply"); }
        catch { setFeedback("Couldn't copy reply"); }
      }} aria-label="Copy reply">Copy</button>
      <button type="button" className="ask-reply-more" ref={button}
        aria-label="More reply actions" title="More reply actions" aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => { setFeedback(""); setOpen((current) => !current); }}>
        <ChevronRightIcon width={14} height={14} aria-hidden />
      </button>
      {open && <div id={id} className="ask-reply-menu" role="group" aria-label="Reply actions">
        <button type="button" ref={firstAction} onClick={async () => {
          close();
          try {
            await navigator.clipboard.writeText(text);
            setFeedback("Copied reply");
          } catch {
            setFeedback("Couldn’t copy reply");
          }
        }}>Copy reply</button>
        {onActivity && <button type="button" onClick={() => { setOpen(false); onActivity(button.current); }}>View activity</button>}
      </div>}
      {feedback && <span className="ask-reply-feedback" role="status">{feedback}</span>}
    </div>
  );
}

function TeamActivity({ events, running }: { events: ExecutionEvent[]; running: boolean }) {
  return (
    <div className="ask-team-activity" aria-live="polite">
      {events.filter((event) => event.outcome !== "dispatching").map((event, index) => {
        let data: { name?: string; slug?: string; parent?: string; assignment?: string; answer?: string; model_route?: { model: string; gateway: string } } = {};
        try { data = JSON.parse(event.details); } catch { /* Older receipts may be plain text. */ }
        const status = event.kind === "approval" && event.outcome === "awaiting_confirmation"
          ? running && index === events.length - 1 ? "Awaiting owner approval" : "Approval requested"
          : event.outcome.replace(/_/g, " ");
        return (
          <article className="ask-team-event" key={index}>
            <div><strong>{data.name ?? data.slug ?? "PAIGE"}</strong><span>{status}</span></div>
            <p>{event.summary}</p>
            {data.assignment && event.outcome === "assigned" && <small>{data.assignment}</small>}
            {data.answer && data.parent !== "PAIGE" && <p>{data.answer}</p>}
            {data.model_route && (
              <small className="ask-team-route">{data.model_route.model} · {data.model_route.gateway}</small>
            )}
          </article>
        );
      })}
    </div>
  );
}

export function AskPaige({ visible, enabledSkills, requestedSkillId, onSkillRequestHandled, onNavigate, onToggleTheme,
  skills, onInstalled, onUseInstalled }: {
  visible: boolean;
  enabledSkills: string[];
  skills: SkillInfo[];
  onInstalled: (skill: InstalledSkill) => void;
  onUseInstalled: (skill: InstalledSkill) => void;
  requestedSkillId: string | null;
  onSkillRequestHandled: () => void;
  onNavigate: (screen: Screen) => void;
  onToggleTheme: () => void;
}) {
  const ask = useAsk();
  const broker = useBroker();
  const [input, setInput] = useState("");
  // A project chosen for a brand-new conversation before any message exists.
  // It is pinned to the conversation record when the first message is sent —
  // never before, so picking a project never creates an empty record.
  const [pendingWorkspace, setPendingWorkspace] = useState<string | null>(null);
  // Pin a draft-chosen project once the first turn has finished: the turn lock
  // is released then, so chat_pin_workspace never blocks the send flow.
  const pendingPinRef = useRef<{ chatId: string; root: string } | null>(null);
  const [mode, setMode] = useState<AskMode>("plan");
  const [leftOpen, setLeftOpen] = useState(() => !window.matchMedia("(max-width: 720px)").matches);
  const [compact, setCompact] = useState(() => window.matchMedia("(max-width: 1020px)").matches);
  const [compactHistory, setCompactHistory] = useState(() => window.matchMedia("(max-width: 720px)").matches);
  const [rightOpen, setRightOpen] = useState(false);
  const [workbenchTab, setWorkbenchTab] = useState<WorkbenchTab>("Changes");
  const [terminalRoots, setTerminalRoots] = useState<string[]>([]);
  const [approvalPreview, setApprovalPreview] = useState<ExecutionEvent | null>(null);
  const [openingApproval, setOpeningApproval] = useState<string | null>(null);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [visualPreviewId, setVisualPreviewId] = useState<string | null>(null);
  const [composerMenu, setComposerMenu] = useState<ComposerMenu | null>(null);
  const [skillQuery, setSkillQuery] = useState("");
  const [skillId, setSkillId] = useState<string | null>(null);
  const [focusSkillComposer, setFocusSkillComposer] = useState(false);
  const selectedSkill = skills.find((skill) => skill.id === skillId && enabledSkills.includes(skill.id));
  const matchingSkills = skills.filter((skill) => enabledSkills.includes(skill.id)
    && `${skill.name} ${skill.description}`.toLowerCase().includes(skillQuery.trim().toLowerCase()));
  useEffect(() => {
    if (composerMenu === "options" || composerMenu === "workflows") {
      document.querySelector<HTMLButtonElement>("#ask-composer-options button:not(:disabled)")?.focus();
    }
  }, [composerMenu]);
  const menuButton = useRef<HTMLButtonElement>(null);
  const workbenchToggle = useRef<HTMLButtonElement>(null);
  const historyToggle = useRef<HTMLButtonElement>(null);
  const workbenchOrigin = useRef<HTMLButtonElement | null>(null);
  const openedLivePlan = useRef<ExecutionEvent | null>(null);
  const [workflowDraft, setWorkflowDraft] = useState<WorkflowTemplate | null>(null);
  const [workflowIntentText, setWorkflowIntentText] = useState<string | null>(null);
  const [visualProposalOnly, setVisualProposalOnly] = useState(false);
  const [workflowChoices, setWorkflowChoices] = useState<WorkflowTemplate[]>([]);
  const [skipIntentText, setSkipIntentText] = useState<string | null>(null);
  const [restartBeforeTests, setRestartBeforeTests] = useState(false);
  const [workflowFile, setWorkflowFile] = useState("");
  const [beforePng, setBeforePng] = useState("");
  const [captureWindow, setCaptureWindow] = useState(false);
  const startupRecovery = useRef<string | null>(null);
  const composerMode = workflowDraft && !visualProposalOnly ? "build" : mode;
  const [inspectTurn, setInspectTurn] = useState<string | null>(null);
  const [chats, setChats] = useState<Chat[]>([]);
  const [activeChatId, setActiveChatId] = useState<string | null>(null);
  // A pending project belongs only to a not-yet-sent draft: once a real
  // conversation is active, the draft context is discarded.
  useEffect(() => { if (activeChatId) setPendingWorkspace(null); }, [activeChatId]);
  const [chatQuery, setChatQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [teamPicker, setTeamPicker] = useState(false);
  const [projectPicker, setProjectPicker] = useState(false);
  const [openOptionsId, setOpenOptionsId] = useState<string | null>(null);
  const [closedProjects, setClosedProjects] = useState<Set<string>>(new Set());
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [projectOptionsRoot, setProjectOptionsRoot] = useState<string | null>(null);
  const [renamingProjectRoot, setRenamingProjectRoot] = useState<string | null>(null);
  const [chatError, setChatError] = useState<string | null>(null);
  const [skillHandoffError, setSkillHandoffError] = useState<string | null>(null);
  const [runningChatId, setRunningChatId] = useState<string | null>(null);
  const [chatPending, setChatPending] = useState(false);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [editingChatId, setEditingChatId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [attachments, setAttachments] = useState<string[]>([]);
  const [contextRefs, setContextRefs] = useState<string[]>([]);
  const [chatModels, setChatModels] = useState(readChatModels);
  const [newChatModel, setNewChatModel] = useState<string | null>(null);
  const savedModel = ask.status?.selection === "manual" ? ask.status.manual_pin : ask.status ? "auto" : null;
  const chatModelOverride = (activeChatId ? chatModels[activeChatId] : newChatModel) ?? null;
  const modelChoice = chatModelOverride ?? savedModel;
  const drafts = useRef(new Map<string, ChatDraft>());
  const [slashOpen, setSlashOpen] = useState(false);
  const [atOpen, setAtOpen] = useState(false);
  const [atQuery, setAtQuery] = useState("");
  const [attachOpen, setAttachOpen] = useState(false);
  const [attachQuery, setAttachQuery] = useState("");
  const [labFiles, setLabFiles] = useState<FileEntry[]>([]);
  const [exportMsg, setExportMsg] = useState<{ id: string; text: string } | null>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const followLatest = useRef(true);
  const [awayFromLatest, setAwayFromLatest] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const screenshotRef = useRef<HTMLInputElement>(null);
  const stylesheetRef = useRef<HTMLInputElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const chatAction = useRef(false);
  const wasVisible = useRef(visible);

  useEffect(() => {
    if (visible && !wasVisible.current) void ask.refresh();
    wasVisible.current = visible;
  }, [visible, ask.refresh]);

  useEffect(() => {
    try { localStorage.setItem(chatModelPreference, JSON.stringify(chatModels)); }
    catch { /* Keep the active selection for this session. */ }
  }, [chatModels]);

  useEffect(() => {
    const drawer = window.matchMedia("(max-width: 1020px)");
    const history = window.matchMedia("(max-width: 720px)");
    const onDrawer = (event: MediaQueryListEvent) => setCompact(event.matches);
    const onHistory = (event: MediaQueryListEvent) => {
      setCompactHistory(event.matches);
      setLeftOpen(!event.matches);
    };
    drawer.addEventListener("change", onDrawer);
    history.addEventListener("change", onHistory);
    return () => {
      drawer.removeEventListener("change", onDrawer);
      history.removeEventListener("change", onHistory);
    };
  }, []);

  const activeChat = useMemo(
    () => chats.find((c) => c.id === activeChatId) ?? null,
    [chats, activeChatId],
  );
  const messages = activeChat?.messages;
  const team = activeChat?.team;
  const specialists = [...(activeChat?.specialists ?? []), ...(team?.members ?? [])];
  const pinnedWorkspace = activeChat?.project_roots[0];
  const authorizedWorkspace = broker.status?.root;
  const noProject = !!activeChat?.no_project;
  const pinnedReady = !pinnedWorkspace || (broker.status?.read && authorizedWorkspace === pinnedWorkspace);
  const savedTask = activeChat?.resume;
  const lastPause = savedTask && activeChat?.execution.slice().reverse().find((event) =>
    event.turn_id === savedTask.turn_id && event.tool === "task_paused");
  const pauseExplanation = lastPause && lastPause.summary.startsWith("I") ? lastPause.summary : undefined;
  const hasArchived = chats.some((c) => c.archived);
  const runVisible = runningChatId !== null && runningChatId === activeChatId;
  const showLiveReply = ask.busy && !ask.settledTurn
    && (runVisible || (chatPending && runningChatId === null && activeChatId === null));
  const startingReply = showLiveReply && ask.activeTurnId === null;
  const lastActivity = ask.liveActivity[ask.liveActivity.length - 1];
  const pendingApproval = runVisible && ask.busy && lastActivity?.kind === "approval"
    && ["awaiting_review", "awaiting_confirmation"].includes(lastActivity.outcome) ? lastActivity : null;
  const requestId = pendingApproval ? readReceipt(pendingApproval).review_id : undefined;
  const lastTool = ask.toolEvents[ask.toolEvents.length - 1];
  const liveMessage = liveRunLabel(lastActivity, lastTool?.summary === "Started" ? lastTool.tool : undefined, startingReply);
  const workspaceAccess = noProject ? "No project selected"
    : !pinnedWorkspace ? "Workspace optional"
    : pinnedReady ? "Authorized"
    : broker.status?.read ? "Wrong workspace authorized"
    : "Access needed";

  const refreshChats = async (): Promise<ChatListResult> => {
    const res = await invoke<ChatListResult>("chat_list");
    setChats(res.chats);
    return res;
  };

  // Apply a draft-chosen project pin once the turn has ended and the lock is
  // free. Failure is surfaced by the chat list staying unpinned; the owner can
  // re-pin from the context control at any time.
  useEffect(() => {
    const pin = pendingPinRef.current;
    if (!pin || ask.busy || activeChatId !== pin.chatId) return;
    pendingPinRef.current = null;
    void invoke<Chat>("chat_pin_workspace", { id: pin.chatId, root: pin.root })
      .then(() => refreshChats())
      .catch(() => {});
  }, [ask.busy, activeChatId]);

  const filteredChats = useMemo(() => {
    const q = chatQuery.trim().toLowerCase();
    return chats.filter(
      (c) => c.archived === showArchived && (!q || c.title.toLowerCase().includes(q) ||
        [...(c.specialists ?? []), ...(c.team?.members ?? [])]
          .some((member) => `${member.name} ${member.slug}`.toLowerCase().includes(q)) ||
        c.messages.some((m) => m.text.toLowerCase().includes(q))),
    );
  }, [chats, chatQuery, showArchived]);
  const projectChats = useMemo(() => {
    const groups = new Map<string, { root: string | null; chats: Chat[] }>();
    for (const chat of filteredChats) {
      const root = chat.project_roots[0] ?? null;
      const key = root ? rootKeyOf(root) : "";
      const existing = groups.get(key);
      if (existing) existing.chats.push(chat);
      else groups.set(key, { root, chats: [chat] });
    }
    return [...groups.entries()];
  }, [filteredChats]);
  const independentChats = projectChats.find(([key]) => key === "")?.[1].chats ?? [];
  const projectGroups = useMemo(() => {
    type ProjectGroup = { root: string; chats: Chat[]; name?: string };
    const groups = new Map<string, ProjectGroup>();
    for (const [, group] of projectChats) {
      if (!group.root) continue;
      groups.set(rootKeyOf(group.root), { root: group.root, chats: group.chats });
    }
    for (const project of projects) {
      const key = rootKeyOf(project.root);
      const existing = groups.get(key);
      if (existing) existing.name = project.name;
      else groups.set(key, { root: project.root, chats: [], name: project.name });
    }
    const merged: Array<[string, ProjectGroup]> = [];
    for (const project of projects) {
      const key = rootKeyOf(project.root);
      const group = groups.get(key);
      if (group) merged.push([key, group]);
    }
    for (const [key, group] of groups) {
      if (!projects.some((project) => rootKeyOf(project.root) === key)) merged.push([key, group]);
    }
    return merged;
  }, [projectChats, projects]);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const res = await invoke<ChatListResult>("chat_list");
        if (!mounted) return;
        setChats(res.chats);
        const byId = res.chats.find((c) => c.id === res.active_id);
        const fallback = res.chats.find((c) => !c.archived);
        startupRecovery.current = byId?.resume?.restart_ready ? byId.id : null;
        setActiveChatId(byId?.id ?? fallback?.id ?? null);
        setShowArchived(byId?.archived ?? false);
      } catch (error) {
        if (mounted) setChatError(String(error));
      } finally {
        if (mounted) setHistoryLoaded(true);
      }
    })();
    return () => { mounted = false; };
  }, []);

  useEffect(() => {
    const id = startupRecovery.current;
    if (!visible || !historyLoaded || !id || id !== activeChatId || ask.busy) return;
    startupRecovery.current = null;
    setRunningChatId(id);
    void ask.resume(id, undefined, false, true).then(started => {
      if (!started) setRunningChatId(null);
    });
  }, [visible, historyLoaded, activeChatId, ask.busy, ask.resume]);

  useEffect(() => {
    if (!ask.settledTurn) return;
    let mounted = true;
    void invoke<ChatListResult>("chat_list").then(async ({ chats }) => {
      if (mounted) {
        await invoke("chat_select");
        await broker.refreshStatus();
      }
      if (mounted) {
        setChats(chats);
        setRunningChatId(null);
      }
    }).catch((error) => {
      if (mounted) {
        setChatError(String(error));
        setRunningChatId(null);
      }
    });
    return () => { mounted = false; };
  }, [ask.settledTurn]);

  useEffect(() => {
    if (!visible) return;
    const focusSearch = (event: KeyboardEvent) => {
      if (!event.ctrlKey || !event.shiftKey || event.key.toLowerCase() !== "f") return;
      event.preventDefault();
      setLeftOpen(true);
      requestAnimationFrame(() => searchRef.current?.focus());
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, [visible]);

  useEffect(() => {
    if (!(atOpen || attachOpen) || !pinnedReady || noProject) return;
    let mounted = true;
    (async () => {
      try {
        const files = await invoke<FileEntry[]>("broker_list_files", { expectedRoot: pinnedWorkspace });
        if (mounted) setLabFiles(files);
      } catch { /* not granted — pickers show the grant hint */ }
    })();
    broker.refreshStatus();
    return () => { mounted = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [atOpen, attachOpen, pinnedWorkspace, pinnedReady, noProject]);

  useLayoutEffect(() => {
    followLatest.current = true;
    setAwayFromLatest(false);
  }, [activeChatId, teamPicker]);

  useLayoutEffect(() => {
    const transcript = transcriptRef.current;
    if (transcript && followLatest.current) transcript.scrollTop = transcript.scrollHeight;
  }, [activeChatId, teamPicker, visible, messages, showLiveReply, startingReply, ask.toolEvents, ask.liveActivity, rightOpen, leftOpen]);

  const onTranscriptScroll = () => {
    const transcript = transcriptRef.current;
    if (!transcript) return;
    const away = transcript.scrollHeight - transcript.clientHeight - transcript.scrollTop > 72;
    followLatest.current = !away;
    setAwayFromLatest(away);
  };

  const jumpToLatest = () => {
    const transcript = transcriptRef.current;
    if (!transcript) return;
    followLatest.current = true;
    transcript.focus({ preventScroll: true });
    transcript.scrollTop = transcript.scrollHeight;
    setAwayFromLatest(false);
  };

  const filteredFiles = useMemo(() => {
    const q = atQuery.toLowerCase();
    return labFiles.filter((f) => f.rel_path.toLowerCase().includes(q)).slice(0, 12);
  }, [labFiles, atQuery]);

  const filteredAttach = useMemo(() => {
    const q = attachQuery.toLowerCase();
    return labFiles.filter((f) => f.rel_path.toLowerCase().includes(q)).slice(0, 12);
  }, [labFiles, attachQuery]);

  const brokerGranted = !!(!noProject && pinnedReady && broker.status?.granted);
  const grantedWorkspace = brokerGranted ? authorizedWorkspace : null;
  const terminalWorkspace = broker.status?.terminal_available ? grantedWorkspace : null;
  useEffect(() => {
    if (!rightOpen || workbenchTab !== "Terminal" || !terminalWorkspace) return;
    setTerminalRoots((roots) => roots.includes(terminalWorkspace) ? roots : [...roots, terminalWorkspace]);
  }, [terminalWorkspace, rightOpen, workbenchTab]);
  const waitingForWorkspace = !!savedTask?.waiting_for_workspace;
  const needsTaskFolder = waitingForWorkspace && (!broker.status?.read || !grantedWorkspace);
  const needsSavedWorkspace = !!savedTask?.root && (grantedWorkspace?.toLowerCase() !== savedTask.root.toLowerCase()
    || broker.status?.read !== savedTask.read || broker.status?.write !== savedTask.write);
  const archivedChat = activeChat?.archived === true;
  useEffect(() => {
    if (!visible || !historyLoaded || !requestedSkillId) return;
    onSkillRequestHandled();
    const skill = skills.find((item) => item.id === requestedSkillId && enabledSkills.includes(item.id));
    if (!skill) {
      setSkillHandoffError("Enable this skill in Settings before using it in chat.");
    } else if (archivedChat) {
      setSkillHandoffError("Unarchive this chat or open another chat to use the skill.");
    } else if (ask.busy || chatPending || runningChatId || savedTask) {
      setSkillHandoffError("Finish or end the current task before choosing a skill.");
    } else if (projectPicker || teamPicker) {
      setSkillHandoffError("Close the current picker before choosing a skill.");
    } else if (workflowDraft === "research_v1" || (workflowDraft === "visual_qa_v1" && !skill.design)) {
      setSkillHandoffError("Finish or clear the workflow draft before choosing a skill.");
    } else {
      setSkillHandoffError(null);
      setSkillId(skill.id);
      setComposerMenu(null);
      if (compact) setRightOpen(false);
      if (compactHistory) setLeftOpen(false);
      setFocusSkillComposer(true);
    }
  }, [visible, historyLoaded, requestedSkillId, onSkillRequestHandled, enabledSkills, archivedChat,
    ask.busy, chatPending, runningChatId, savedTask, projectPicker, teamPicker, workflowDraft, compact, compactHistory]);
  useEffect(() => {
    if (!focusSkillComposer) return;
    setFocusSkillComposer(false);
    if (visible && selectedSkill) inputRef.current?.focus();
  }, [focusSkillComposer, visible, selectedSkill]);
  useEffect(() => {
    if (visible) void broker.refreshStatus();
  }, [visible, broker.refreshStatus]);

  const [scopeMode, setScopeMode] = useState<"observe" | "selected" | "full" | null>(null);
  useEffect(() => {
    if (!visible) return;
    void invoke<{ mode: "observe" | "selected" | "full" }>("scope_status")
      .then((status) => setScopeMode(status.mode))
      .catch(() => setScopeMode(null));
  }, [visible]);
  const scopeLabel = scopeMode === "observe" ? "Observe"
    : scopeMode === "full" ? "Full access"
    : scopeMode === "selected" ? "Selected scope"
      : "Scope unavailable";
  const scopeNote = scopeMode === "observe"
    ? "Read-only. No writes, terminal or effectful tools."
    : scopeMode === "full"
      ? "Ordinary operations inside your granted roots, within your Windows account's own permissions."
      : scopeMode === "selected"
        ? "Only the roots granted in Settings under Computer Access."
        : "Computer Access status could not be read. No effectful operations are authorized.";

  const withChatAction = async (action: () => Promise<void>) => {
    if (chatAction.current) return;
    chatAction.current = true;
    setChatPending(true);
    setChatError(null);
    try {
      await action();
    } catch (error) {
      setChatError(String(error));
    } finally {
      chatAction.current = false;
      setChatPending(false);
    }
  };

  const refreshProjects = async () => {
    try { setProjects(await invoke<ProjectEntry[]>("project_list")); } catch { /* the registry is optional */ }
  };
  useEffect(() => { if (visible) void refreshProjects(); }, [visible]);

  const addProject = () => void withChatAction(async () => {
    const root = await invoke<string | null>("project_pick_folder");
    if (!root) return;
    await invoke("project_grant", { request: { name: folderName(root), root } });
    await refreshProjects();
  });

  const renameProject = (root: string, name: string) => void withChatAction(async () => {
    const project = projects.find((entry) => rootKeyOf(entry.root) === rootKeyOf(root));
    if (!project) throw new Error("This project is no longer in PAIGE's list.");
    await invoke("project_rename", { id: project.id, name });
    await refreshProjects();
  });

  const removeProject = (root: string) => void withChatAction(async () => {
    const project = projects.find((entry) => rootKeyOf(entry.root) === rootKeyOf(root));
    if (!project) throw new Error("This project is no longer in PAIGE's list.");
    await invoke("project_remove", { id: project.id });
    await refreshProjects();
  });

  const send = () => void withChatAction(async () => {
    const text = input.trim();
    if (!historyLoaded || !text || projectPicker || ask.busy || runningChatId || activeChat?.archived) return;
    const detectedIntent = !savedTask && !workflowDraft && (!selectedSkill || selectedSkill.design) && skipIntentText !== text
      ? resolveChatIntent(text, { skills: skills.filter((skill) => enabledSkills.includes(skill.id)),
        specialists: activeChat?.specialists ?? [], attachments: [...attachments, ...contextRefs] })
      : null;
    const intent = !selectedSkill || (detectedIntent?.kind === "workflow" && detectedIntent.template === "visual_qa_v1")
      ? detectedIntent : null;
    if (intent?.kind === "unavailable") {
      setChatError(intent.message);
      return;
    }
    if (intent?.kind === "choose_workflow") {
      setWorkflowChoices(intent.templates);
      return;
    }

    const automatic = intent?.kind === "workflow" ? intent : null;
    const template = workflowDraft ?? automatic?.template;
    const fromChat = !!automatic || workflowIntentText === text;
    const capturesWindow = automatic ? automatic.captureWindow === true : captureWindow;
    const before = beforePng.trim() || automatic?.before || "";
    const css = workflowFile.trim() || automatic?.css || "";
    if (automatic) {
      setWorkflowDraft(automatic.template);
      setWorkflowIntentText(text);
      setVisualProposalOnly(automatic.proposalOnly === true);
      setBeforePng(before);
      setCaptureWindow(capturesWindow);
      setWorkflowFile(css);
      setRestartBeforeTests(false);
    }
    setWorkflowChoices([]);
    if (template && (!grantedWorkspace || !broker.status?.read || !broker.status.write || (fromChat && !pinnedWorkspace))) {
      setChatError(pinnedWorkspace ? "Authorize this chat’s pinned project before starting the workflow."
        : "Choose a project for this workflow. Your request stays in the composer.");
      if (!pinnedWorkspace) setProjectPicker(true);
      return;
    }
    const visual = template === "visual_qa_v1";
    const proposalOnly = visual && (automatic?.proposalOnly ?? (fromChat && visualProposalOnly));
    if (visual && ((!capturesWindow && !before) || !css)) {
      requestAnimationFrame(() => (capturesWindow || before ? stylesheetRef.current : screenshotRef.current)?.focus());
      return;
    }
    if (automatic && mode === "plan" && !proposalOnly) {
      requestAnimationFrame(() => inputRef.current?.focus());
      return;
    }
    const isVisualInput = (path: string) => visual && [before, css].some((selected) =>
      path.replace(/\\/g, "/").toLowerCase() === selected.replace(/\\/g, "/").toLowerCase());
    const composer: ComposerState = savedTask
      ? { mode: "plan", attachments: [], contextRefs: [], modelChoice: chatModelOverride }
      : { mode: template ? "build" : composerMode,
          attachments: attachments.filter((path) => !isVisualInput(path)),
          contextRefs: contextRefs.filter((path) => !isVisualInput(path)),
          modelChoice: chatModelOverride, skillId: selectedSkill?.id ?? (intent?.kind === "skill" ? intent.id : undefined),
          specialistHandle: intent?.kind === "specialist" ? intent.slug : undefined,
          workflow: template && grantedWorkspace
            ? { template, workspace_root: grantedWorkspace, word_report: template === "research_v1", selected_from_chat: fromChat,
                restart_before_tests: template === "engineering_v1" && restartBeforeTests,
                file_scope: template === "engineering_v1" && restartBeforeTests ? css || undefined : undefined,
                visual: visual ? { before: capturesWindow ? "" : before, css,
                  proposal_only: proposalOnly, capture_window: capturesWindow } : undefined } : undefined };
    setRunningChatId(activeChat?.id ?? null);
    setSlashOpen(false);
    setAtOpen(false);
    setAttachOpen(false);
    setComposerMenu(null);
    setApprovalPreview(null);
    setOpeningApproval(null);
    setApprovalError(null);
    const started = await ask.send(text, composer, activeChat?.id ?? null);
    if (started?.chat_id) {
      const startedId = started.chat_id;
      // A project chosen before this first message is pinned when the turn
      // ends — chat_pin_workspace needs the turn lock that the running turn
      // already holds, so an immediate pin would block the send flow.
      if (!activeChatId && pendingWorkspace) {
        pendingPinRef.current = { chatId: startedId, root: pendingWorkspace };
        setPendingWorkspace(null);
      }
      if (!activeChatId && newChatModel) {
        setChatModels((previous) => ({ ...previous, [startedId]: newChatModel }));
        setNewChatModel(null);
      }
      followLatest.current = true;
      setAwayFromLatest(false);
      setProjectPicker(false);
      setActiveChatId(started.chat_id);
      setRunningChatId(started.chat_id);
      if (template) {
        if (!fromChat) setMode("build");
        setWorkflowDraft(null);
        setWorkflowIntentText(null);
        setBeforePng("");
        setCaptureWindow(false);
        setWorkflowFile("");
      }
      if (visual) {
        setAttachments((paths) => paths.filter((path) => !isVisualInput(path)));
        setContextRefs((paths) => paths.filter((path) => !isVisualInput(path)));
      }
      setSkipIntentText(null);
      setInput("");
      setSkillId(null);
      setShowArchived(false);
      setChatQuery("");
    } else {
      setRunningChatId(null);
    }
    await refreshChats();
    await broker.refreshStatus();
  });

  const resumeTask = (useOpenrouterFree = false) => void withChatAction(async () => {
    if (!activeChat || !savedTask || ask.busy || savedTask.blocked || activeChat.archived || needsTaskFolder) return;
    setRunningChatId(activeChat.id);
    const workspace = waitingForWorkspace ? grantedWorkspace ?? undefined : undefined;
    if (!(await ask.resume(activeChat.id, workspace, useOpenrouterFree))) setRunningChatId(null);
    await refreshChats();
  });

  const restoreTaskWorkspace = () => {
    if (!savedTask?.root) return;
    const root = savedTask.root.toLowerCase();
    const preauthorized = broker.status?.allowed_roots.some((path) => {
      const allowed = path.toLowerCase().replace(/[\\/]+$/, "");
      return root === allowed || root.startsWith(allowed + "\\");
    });
    if (preauthorized) void broker.grantWorkspace(savedTask.root, savedTask.read, savedTask.write);
    else openProject(savedTask.root, true);
  };

  const acceptWorkflowArtifact = () => void withChatAction(async () => {
    if (!activeChat || !inspectTurn || ask.busy) return;
    await invoke("workflow_accept_artifact", { chatId: activeChat.id, turnId: inspectTurn });
    await refreshChats();
  });

  const retryVisualTest = () => void withChatAction(async () => {
    if (!activeChat || !inspectTurn || ask.busy) return;
    try {
      await invoke("workflow_retry_visual_test", { chatId: activeChat.id, turnId: inspectTurn });
    } finally {
      await refreshChats();
    }
  });

  const compareVisual = (path: string | null) => void withChatAction(async () => {
    if (!activeChat || !inspectTurn || ask.busy) return;
    await invoke("workflow_compare_visual", { chatId: activeChat.id, turnId: inspectTurn, path, captureWindow: path === null });
    await refreshChats();
  });

  const saveWordCopy = (name: string) => void withChatAction(async () => {
    if (!activeChat || !inspectTurn) return;
    const bytes = await invoke<ArrayBuffer>("workflow_read_word", { chatId: activeChat.id, turnId: inspectTurn });
    const file = new Blob([bytes], { type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" });
    const url = URL.createObjectURL(file);
    const download = document.createElement("a");
    download.href = url;
    download.download = name;
    document.body.append(download);
    download.click();
    download.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 5_000);
  });

  const endTask = () => void withChatAction(async () => {
    if (!activeChat || !savedTask || ask.busy) return;
    await invoke("ask_end_task", { chatId: activeChat.id });
    ask.clearError();
    await refreshChats();
  });

  const evidenceOf = (m: ChatMessage): ExecutionEvent[] =>
    activeChat ? activeChat.execution.filter((e) => m.turn_id && e.turn_id === m.turn_id && e.kind !== "conversation") : [];

  const connectorReceiptOf = (message: ChatMessage) => {
    const isLookupReceipt = (event: ExecutionEvent) => event.turn_id === message.turn_id &&
      event.kind === "tool" && ((event.tool === "linear_get_issue" && event.gateway === "linear-mcp")
        || (event.tool === "notion_fetch_page" && event.gateway === "notion-mcp")
        || (event.tool === "github_issue_read" && event.gateway === "github-mcp")
        || (event.tool === "supabase_log_count" && event.gateway === "supabase-mcp")
        || (event.tool === "web_search" && event.gateway === "omniroute-web")
        || (event.tool === "skills_search" && event.gateway === "skills-discovery"));
    return evidenceOf(message).find(isLookupReceipt) ??
      (runVisible ? ask.liveActivity.find(isLookupReceipt) : undefined);
  };

  const teamActivityOf = (message: ChatMessage): ExecutionEvent[] => {
    const live = runVisible ? ask.liveActivity.filter((event) => event.turn_id === message.turn_id) : [];
    return mergeWorkflowEvidence(evidenceOf(message), live).filter((event) => event.kind === "handoff");
  };

  const inspectedWorkflow = activeChat?.workflow_runs?.find((run) => run.turn_id === inspectTurn);
  const savedEvidence = activeChat?.execution.filter((event) => !inspectTurn || event.turn_id === inspectTurn) ?? [];
  const liveEvidence = runVisible ? ask.liveActivity.filter((event) => !inspectTurn || event.turn_id === inspectTurn) : [];
  const evidence = mergeWorkflowEvidence(savedEvidence, liveEvidence);
  const timeline = evidence.flatMap((event) => {
    const step = runStep(event);
    return step ? [{ event, step }] : [];
  }).reverse();
  const timelineEvents = new Set(timeline.map(({ event }) => event));
  const otherReceipts = evidence.filter((event) => !timelineEvents.has(event));
  const progressTurn = showLiveReply ? ask.activeTurnId
    : savedTask?.turn_id ?? (messages && messages[messages.length - 1]?.turn_id);
  const progressResults = progressTurn ? activeChat?.execution.filter((event) => event.turn_id === progressTurn)
    .map((event) => ({ event, step: runStep(event) })).reverse() : undefined;
  const latestResult = progressResults?.find(({ step }) => step && step.tab !== "Activity");
  const lastVerified = progressResults?.find(({ step }) => step?.tone === "success");
  const livePlan = runVisible ? ask.liveActivity.slice().reverse().find(isWorkbenchPlan) : undefined;
  useEffect(() => {
    if (!livePlan || openedLivePlan.current === livePlan) return;
    openedLivePlan.current = livePlan;
    if (compact) return;
    setInspectTurn(livePlan.turn_id ?? null);
    setApprovalPreview(null);
    setWorkbenchTab("Plan");
    setRightOpen(true);
  }, [livePlan, compact]);
  const showEvidence = (turnId: string | null, tab: WorkbenchTab = "Activity", origin: HTMLButtonElement | null = null) => {
    workbenchOrigin.current = origin;
    setInspectTurn(turnId);
    setApprovalPreview(null);
    setWorkbenchTab(tab);
    setRightOpen(true);
    requestAnimationFrame(() => document.getElementById("workbench-tab-" + tab)?.focus());
  };

  const reviewApproval = (origin?: HTMLButtonElement) => {
    if (!pendingApproval) return;
    workbenchOrigin.current = origin ?? null;
    setInspectTurn(pendingApproval.turn_id ?? null);
    setApprovalPreview(pendingApproval);
    setApprovalError(null);
    setWorkbenchTab("Changes");
    setRightOpen(true);
    requestAnimationFrame(() => {
      document.getElementById("ask-approval-review")?.scrollIntoView({ block: "nearest" });
      document.getElementById("workbench-tab-Changes")?.focus({ preventScroll: true });
    });
  };

  const openConfirmation = async () => {
    if (typeof requestId !== "string" || !pendingApproval?.turn_id || openingApproval === requestId) return;
    if (pendingApproval.tool === "visual_inspection" && visualPreviewId !== requestId) return;
    setApprovalError(null);
    setOpeningApproval(requestId);
    try {
      await invoke("ask_open_confirmation", { turnId: pendingApproval.turn_id, requestId });
    } catch (error) {
      setApprovalError(String(error));
      setOpeningApproval(null);
    }
  };

  const closeWorkbench = () => {
    setRightOpen(false);
    const approvalTrigger = approvalPreview && pendingApproval?.outcome === "awaiting_review"
      ? document.getElementById("ask-approval-review") : null;
    const origin = workbenchOrigin.current;
    workbenchOrigin.current = null;
    const target = origin?.isConnected ? origin : approvalTrigger ?? workbenchToggle.current;
    requestAnimationFrame(() => target?.focus({ preventScroll: true }));
  };

  const handleInputChange = (value: string) => {
    if (workflowIntentText && workflowIntentText !== value.trim()) {
      setWorkflowDraft(null);
      setWorkflowIntentText(null);
      setBeforePng("");
      setCaptureWindow(false);
      setWorkflowFile("");
    }
    setWorkflowChoices([]);
    if (skipIntentText !== value.trim()) setSkipIntentText(null);
    setInput(value);
    setSlashOpen(value.startsWith("/") && !value.includes(" "));
    const atMatch = value.match(/@([\w./-]*)$/);
    if (atMatch) { setAtOpen(true); setAtQuery(atMatch[1]); } else { setAtOpen(false); }
  };

  const applySlash = (cmd: { cmd: string; prompt: string }) => {
    handleInputChange(cmd.prompt);
    setSkipIntentText(cmd.prompt.trim());
    setSlashOpen(false);
    inputRef.current?.focus();
  };

  const addContextRef = (path: string) => {
    if (!contextRefs.includes(path)) setContextRefs((prev) => [...prev, path]);
    setInput((prev) => prev.replace(/@([\w./-]*)$/, ""));
    setAtOpen(false);
    inputRef.current?.focus();
  };

  const addAttachment = (path: string) => {
    if (!attachments.includes(path)) setAttachments((prev) => [...prev, path]);
    setAttachOpen(false);
  };

  const changeModel = async (choice: string) => {
    if (activeChatId) setChatModels((previous) => ({ ...previous, [activeChatId]: choice }));
    else setNewChatModel(choice);
  };

  const gatewaySummary = ask.status
    ? ask.status.gateways.map((g) => `${g.label}: ${g.reachable ? (g.authenticated ? "ready" : "auth needed") : "offline"}`).join(" · ")
    : "probing…";

  const omnirouteHealth = ask.status?.gateways.find((g) => g.gateway === "omniroute");
  const openrouterHealth = ask.status?.gateways.find((g) => g.gateway === "openrouter-direct");
  const productionReady = !!(omnirouteHealth?.authenticated || openrouterHealth?.authenticated);

  // --- Conversations (left panel) ------------------------------------------

  const rememberDraft = () => {
    if (activeChatId) drafts.current.set(activeChatId, {
      input, mode, skillId, attachments, contextRefs, workflowDraft, workflowIntentText,
      workflowChoices, skipIntentText, visualProposalOnly, restartBeforeTests, workflowFile, beforePng, captureWindow,
    });
  };

  const restoreDraft = (id: string | null) => {
    const draft = id ? drafts.current.get(id) : undefined;
    setInput(draft?.input ?? "");
    setMode(draft?.mode ?? "plan");
    setSkillId(draft?.skillId ?? null);
    if (!draft) return;
    setAttachments(draft.attachments);
    setContextRefs(draft.contextRefs);
    setWorkflowDraft(draft.workflowDraft);
    setWorkflowIntentText(draft.workflowIntentText);
    setWorkflowChoices(draft.workflowChoices);
    setSkipIntentText(draft.skipIntentText);
    setVisualProposalOnly(draft.visualProposalOnly);
    setRestartBeforeTests(draft.restartBeforeTests);
    setWorkflowFile(draft.workflowFile);
    setBeforePng(draft.beforePng);
    setCaptureWindow(draft.captureWindow);
  };

  const clearComposer = () => {
    setProjectPicker(false);
    setLabFiles([]);
    setWorkflowDraft(null);
    setVisualProposalOnly(false);
    setRestartBeforeTests(false);
    setWorkflowIntentText(null);
    setWorkflowChoices([]);
    setSkipIntentText(null);
    setBeforePng("");
    setCaptureWindow(false);
    setWorkflowFile("");
    setSkillId(null);
    setComposerMenu(null);
    setApprovalPreview(null);
    setOpeningApproval(null);
    setApprovalError(null);
    setInput("");
    setAttachments([]);
    setContextRefs([]);
    setAttachOpen(false);
    setAtOpen(false);
    setSlashOpen(false);
    setEditingChatId(null);
    setOpenOptionsId(null);
    setExportMsg(null);
  };

  const newChat = () => {
    if (!historyLoaded || chatAction.current || ask.busy) return;
    rememberDraft();
    if (compactHistory) setLeftOpen(false);
    setNewChatModel(null);
    setActiveChatId(null);
    setPendingWorkspace(null);
    setTeamPicker(false);
    setInspectTurn(null);
    setRightOpen(false);
    setShowArchived(false);
    setChatQuery("");
    clearComposer();
    setMode("plan");
    // An independent conversation opens immediately: no project palette, no
    // inherited project, and no persistent record until the first message is
    // actually sent (ask.send creates the conversation record).
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  const openProject = (folder?: string, authorizeOnly = false) => void withChatAction(async () => {
    if (ask.busy) return;
    const root = folder ?? await invoke<string | null>("project_pick_folder");
    if (!root) return;
    const grant = await invoke<BrokerStatus>("project_open_folder", { root });
    if (!grant.read || !grant.root) throw Error("PAIGE couldn't authorize the selected folder.");
    if (!authorizeOnly && !activeChatId) {
      // A brand-new conversation keeps its draft: choosing a project holds it
      // as the pending workspace and pins it when the first message is sent.
      // No persistent record is created merely by picking a project.
      setPendingWorkspace(grant.root);
      setProjectPicker(false);
      await broker.refreshStatus();
      requestAnimationFrame(() => inputRef.current?.focus());
      return;
    }
    if (!authorizeOnly) {
      const chat = await invoke<Chat>("chat_pin_workspace", { id: activeChatId, root: grant.root });
      if (!activeChatId && newChatModel) {
        setChatModels((previous) => ({ ...previous, [chat.id]: newChatModel }));
        setNewChatModel(null);
      }
      setChats((previous) => [chat, ...previous.filter((item) => item.id !== chat.id)]);
      setActiveChatId(chat.id);
      setShowArchived(false);
      setProjectPicker(false);
      setLabFiles([]);
      setAttachments([]);
      setContextRefs([]);
      rememberFolder(grant.root);
    }
    await broker.refreshStatus();
    requestAnimationFrame(() => inputRef.current?.focus());
  });

  const openTeamPicker = () => {
    setProjectPicker(false);
    setRightOpen(false);
    setTeamPicker(true);
  };

  const acceptTeam = (chat: Chat) => {
    setChats((previous) => [chat, ...previous.filter((item) => item.id !== chat.id)]);
    setActiveChatId(chat.id);
    setShowArchived(false);
    setChatQuery("");
    setTeamPicker(false);
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  const selectChat = (id: string) => void withChatAction(async () => {
    if (activeChatId === id) {
      if (compactHistory) setLeftOpen(false);
      if (compact) setRightOpen(false);
      requestAnimationFrame(() => transcriptRef.current?.focus());
      return;
    }
    await invoke("chat_select", { id });
    rememberDraft();
    setActiveChatId(id);
    setInspectTurn(null);
    clearComposer();
    restoreDraft(id);
    if (compact) setRightOpen(false);
    if (compactHistory) setLeftOpen(false);
    await broker.refreshStatus();
    requestAnimationFrame(() => transcriptRef.current?.focus({ preventScroll: true }));
  });

  const chooseWorkflow = (template: WorkflowTemplate, fromChat = false) => {
    setWorkflowChoices([]);
    setWorkflowIntentText(fromChat ? input.trim() : null);
    setVisualProposalOnly(fromChat && template === "visual_qa_v1" && requestsVisualProposal(input));
    setRestartBeforeTests(false);
    setWorkflowFile("");
    setBeforePng("");
    setCaptureWindow(false);
    setWorkflowDraft(template);
    setComposerMenu(null);
    setChatError(null);
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  const startRename = (chat: Chat) => {
    setOpenOptionsId(null);
    setEditingChatId(chat.id);
    setRenameValue(chat.title);
  };

  const finishRename = () => {
    const inHeader = editingChatId === activeChatId;
    setEditingChatId(null);
    if (inHeader) requestAnimationFrame(() => document.getElementById("ask-header-rename")?.focus());
  };

  const confirmRename = () => void withChatAction(async () => {
    const value = renameValue.trim();
    if (!editingChatId || !value) return;
    await invoke("chat_rename", { id: editingChatId, title: value });
    finishRename();
    await refreshChats();
  });

  const renameEditor = () => (
    <form className="ask-rename-form" onSubmit={(event) => {
      event.preventDefault();
      confirmRename();
    }} onKeyDown={(event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        finishRename();
      }
    }}>
      <input
        id="ask-chat-rename"
        className="ask-chat-rename"
        aria-label="Conversation title"
        value={renameValue}
        onChange={(event) => setRenameValue(event.target.value)}
        onFocus={(event) => event.target.select()}
        autoFocus
      />
      <Button type="submit" variant="primary" size="sm" disabled={chatPending || !renameValue.trim()}>Save</Button>
      <Button type="button" variant="ghost" size="sm" onClick={finishRename}>Cancel</Button>
    </form>
  );

  const setArchived = (id: string | null, archived: boolean) => void withChatAction(async () => {
    if (!id) return;
    await invoke("chat_archive", { id, archived });
    setOpenOptionsId(null);
    if (id === activeChatId) setShowArchived(archived);
    else if (!archived && chats.filter((chat) => chat.archived).length === 1) setShowArchived(false);
    await refreshChats();
  });

  const deleteChat = (id: string) => void withChatAction(async () => {
    if (!id) return;
    if (!confirm("Delete this chat and its history? This cannot be undone.")) return;
    await invoke("chat_delete", { id });
    drafts.current.delete(id);
    setChatModels((previous) => Object.fromEntries(Object.entries(previous).filter(([chatId]) => chatId !== id)));
    const res = await refreshChats();
    if (id === activeChatId) {
      const next = res.chats.find((c) => c.id === res.active_id);
      setActiveChatId(next?.id ?? null);
      setShowArchived(next?.archived ?? false);
      clearComposer();
      restoreDraft(next?.id ?? null);
      setInspectTurn(null);
      if (compact) setRightOpen(false);
      await broker.refreshStatus();
      requestAnimationFrame(() => transcriptRef.current?.focus({ preventScroll: true }));
    } else if (!res.chats.some((chat) => chat.archived)) {
      setShowArchived(false);
    }
  });

  const exportChat = (id: string) => void withChatAction(async () => {
    const text = await invoke<string>("chat_export", { id });
    try {
      await navigator.clipboard.writeText(text);
      setExportMsg({ id, text: "Chat JSON copied to clipboard." });
    } catch {
      setExportMsg({ id, text: "Clipboard unavailable." });
    }
  });

  const renderChatRows = (chats: Chat[]) => chats.map((c) => (
    <div
      key={c.id}
      className="ask-chat-row"
      onContextMenu={(event) => { event.preventDefault(); setOpenOptionsId(c.id); }}
      onKeyDown={(event) => { if (event.key === "Escape") setOpenOptionsId(null); }}
    >
      <button
        type="button"
        className={`ask-chat-item${c.id === activeChatId ? " active" : ""}`}
        onClick={() => selectChat(c.id)}
        aria-current={c.id === activeChatId ? "true" : undefined}
        disabled={chatPending || teamPicker}
      >
        {c.archived ? <FolderIcon className="ask-chat-item-icon" width={14} height={14} aria-hidden /> : <FileTextIcon className="ask-chat-item-icon" width={14} height={14} aria-hidden />}
        <span className="ask-chat-item-text">
          <span className="ask-chat-item-title">{c.title}</span>
          <span className="ask-chat-item-meta">{c.team ? `Team · ${c.team.members.length} specialists` : `${c.messages.length} messages · ${fmtTs(c.updated_at)}`}</span>
        </span>
        {c.id === activeChatId && pendingApproval && <span className="ask-project-approval" aria-label="Approval required" />}
      </button>
      <button type="button" className="ask-chat-more" disabled={teamPicker}
        aria-label={`Options for ${c.title}`} aria-expanded={openOptionsId === c.id}
        onClick={() => setOpenOptionsId((previous) => previous === c.id ? null : c.id)}>⋯</button>
      {openOptionsId === c.id && (
        <div className="ask-chat-options">
          <Button variant="ghost" size="sm" onClick={() => startRename(c)} disabled={chatPending}>Rename</Button>
          <Button variant="ghost" size="sm" onClick={() => setArchived(c.id, !c.archived)} disabled={ask.busy || chatPending}>{c.archived ? "Unarchive" : "Archive"}</Button>
          <Button variant="ghost" size="sm" onClick={() => exportChat(c.id)} disabled={chatPending}>Export</Button>
          <Button variant="ghost" size="sm" onClick={() => deleteChat(c.id)} disabled={ask.busy || chatPending}>Delete</Button>
          {exportMsg?.id === c.id && <div className="ask-export-msg" role="status">{exportMsg.text}</div>}
        </div>
      )}
    </div>
  ));

  return (
    <div className="ask" onKeyDown={(event) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (compactHistory && leftOpen) {
        setLeftOpen(false);
        historyToggle.current?.focus();
      } else if (rightOpen && !teamPicker) closeWorkbench();
    }}>
      <header className="ask-brandbar">
        <div className="ask-brand"><PaigeCommandMark plated={false} label={null} /><strong>PAIGE</strong> <span>Desktop</span></div>
        <Button size="sm" variant="ghost" onClick={onToggleTheme}>Switch theme</Button>
      </header>
      <div className="ask-topbar">
        <button
          id="ask-left-toggle"
          ref={historyToggle}
          type="button"
          className="pd-button pd-button-secondary pd-button-sm"
          onClick={() => {
            if (!leftOpen && compactHistory) setRightOpen(false);
            setLeftOpen(!leftOpen);
            if (!leftOpen) requestAnimationFrame(() => searchRef.current?.focus());
          }}
          title="Toggle the left sidebar"
          aria-expanded={leftOpen}
          aria-controls="ask-left-panel"
        >
          {leftOpen ? <ChevronLeftIcon width={14} height={14} aria-hidden /> : <ChevronRightIcon width={14} height={14} aria-hidden />}
          Conversations
        </button>
        <div className="ask-topbar-conversation">
          <span className="ask-scope-chip" title={`Computer Access: ${scopeLabel} ${scopeNote}`}
            data-mode={scopeMode ?? ""}>{scopeLabel}</span>
          {pinnedWorkspace && <span className="ask-breadcrumb" title={pinnedWorkspace}>
            <FolderIcon width={14} height={14} aria-hidden /> <span>{folderName(pinnedWorkspace)}</span>
            <ChevronRightIcon width={12} height={12} aria-hidden />
          </span>}
          {activeChat && editingChatId === activeChat.id ? renameEditor() : <>
            <h1 className="ask-topbar-title" title={activeChat?.title}>{activeChat?.title ?? "Ask PAIGE"}</h1>
            {activeChat && <Button id="ask-header-rename" type="button" variant="ghost" size="sm"
              aria-label="Rename conversation" onClick={() => startRename(activeChat)} disabled={chatPending}>
              Rename
            </Button>}
          </>}
        </div>
        <button id="ask-right-toggle" type="button" ref={workbenchToggle} className="ask-workbench-toggle"
          onClick={() => {
            if (rightOpen) closeWorkbench();
            else {
              if (compactHistory) setLeftOpen(false);
              setRightOpen(true);
              requestAnimationFrame(() => document.getElementById("workbench-tab-" + workbenchTab)?.focus());
            }
          }}
          disabled={teamPicker || projectPicker}
          aria-expanded={rightOpen} aria-controls="ask-right-panel">
          <ActivityIcon width={14} height={14} aria-hidden /> Workbench
        </button>
      </div>

      <div className="ask-layout">
        {/* LEFT — Conversations */}
        {compactHistory && leftOpen && <button type="button" className="ask-history-scrim" tabIndex={-1}
          aria-label="Close conversations" onClick={() => { setLeftOpen(false); historyToggle.current?.focus(); }} />}
        {leftOpen && (
          <aside className="ask-left" id="ask-left-panel" aria-label="Projects and conversations">
            <div className="ask-left-head">
              <button type="button" className="ask-new-chat" onClick={newChat} disabled={!historyLoaded || chatPending || ask.busy || teamPicker}>
                <NewChatIcon width={16} height={16} aria-hidden />
                New chat
              </button>
              <button type="button" className="ask-add-project" onClick={addProject} disabled={chatPending || ask.busy || teamPicker}>
                <FolderIcon width={16} height={16} aria-hidden />
                Add project
              </button>
            </div>
            <input
              id="ask-chat-search"
              ref={searchRef}
              className="ask-chat-search"
              placeholder="Search conversations…"
              value={chatQuery}
              onChange={(e) => setChatQuery(e.target.value)}
              title="Search chat titles and messages"
              aria-label="Search chat titles and messages"
              aria-keyshortcuts="Control+Shift+F"
            />
            {hasArchived && <div className="ask-left-actions">
              <Button variant="secondary" size="sm" aria-pressed={!showArchived} onClick={() => setShowArchived(false)}>Saved</Button>
              <Button variant="secondary" size="sm" aria-pressed={showArchived} onClick={() => setShowArchived(true)}>Archived</Button>
            </div>}
            {editingChatId && editingChatId !== activeChat?.id && renameEditor()}
            <div className="ask-left-list">
              {filteredChats.length === 0 && (
                <div className="ask-sidepanel-empty">{chatQuery ? "No matching conversations." : showArchived ? "No archived conversations." : "No saved conversations."}</div>
              )}
              {independentChats.length > 0 && <div className="ask-project-label">Chats</div>}
              {renderChatRows(independentChats)}
              {projectGroups.length > 0 && <>
                <div className="ask-project-label">Projects</div>
                {projectGroups.map(([key, group]) => (
                  <details className="ask-project-group" key={key + (chatQuery.trim() ? ":search" : "")}
                    open={!closedProjects.has(key)}
                    onToggle={(event) => setClosedProjects((previous) => {
                      const next = new Set(previous);
                      const target = event.target as HTMLDetailsElement;
                      if (target.open) next.delete(key); else next.add(key);
                      return next;
                    })}>
                    <summary title={group.root}>
                      <FolderIcon width={15} height={15} aria-hidden />
                      <span>{group.name ?? folderName(group.root)}</span>
                      <button type="button" className="ask-chat-more ask-project-more" tabIndex={-1}
                        aria-label={`Options for ${group.name ?? folderName(group.root)}`}
                        aria-expanded={projectOptionsRoot === group.root}
                        onClick={(event) => { event.preventDefault(); event.stopPropagation(); setProjectOptionsRoot((previous) => previous === group.root ? null : group.root); }}>⋯</button>
                      <ChevronRightIcon width={13} height={13} aria-hidden />
                    </summary>
                    {projectOptionsRoot === group.root && (
                      <div className="ask-chat-options ask-project-options">
                        {renamingProjectRoot === group.root ? (
                          <form className="ask-project-rename" onSubmit={(event) => {
                            event.preventDefault();
                            const field = event.currentTarget.elements.namedItem("project-name") as HTMLInputElement | null;
                            const name = field?.value.trim() ?? "";
                            if (!name) return;
                            void renameProject(group.root, name);
                            setRenamingProjectRoot(null);
                          }}>
                            <input name="project-name" defaultValue={group.name ?? folderName(group.root)} maxLength={80} autoFocus
                              aria-label="Project name"
                              onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); setRenamingProjectRoot(null); } }} />
                            <Button size="sm" type="submit">Save</Button>
                            <Button size="sm" variant="ghost" onClick={() => setRenamingProjectRoot(null)}>Cancel</Button>
                          </form>
                        ) : <>
                          <Button variant="ghost" size="sm" onClick={() => { setRenamingProjectRoot(group.root); setProjectOptionsRoot(null); }}>Rename project</Button>
                          <Button variant="ghost" size="sm" onClick={() => {
                            if (window.confirm(`Remove "${group.name ?? folderName(group.root)}" from PAIGE? The folder stays on your computer and its conversations are kept.`)) {
                              setProjectOptionsRoot(null);
                              void removeProject(group.root);
                            }
                          }}>Remove from PAIGE</Button>
                        </>}
                      </div>
                    )}
                    <div className="ask-project-chats">
                      {renderChatRows(group.chats)}
                      {group.chats.length === 0 && <div className="ask-sidepanel-empty">No conversations in this project yet.</div>}
                    </div>
                  </details>
                ))}
              </>}
            </div>
            <nav className="ask-left-nav" aria-label="Ask PAIGE">
              <button type="button" onClick={() => { if (compactHistory) setLeftOpen(false); onNavigate("projects"); }}><FolderIcon aria-hidden /> Projects</button>
              <button type="button" onClick={() => { if (compactHistory) setLeftOpen(false); onNavigate("models"); }}><CpuIcon aria-hidden /> Models &amp; Connections</button>
              <button type="button" onClick={() => { if (compactHistory) setLeftOpen(false); onNavigate("settings"); }}><SettingsIcon aria-hidden /> Settings</button>
            </nav>
          </aside>
        )}

        {/* CENTER — Ask PAIGE */}
        <main className="ask-main" inert={(compact && rightOpen && !teamPicker) || (compactHistory && leftOpen)}>
          {skillHandoffError && <div className="ask-error-banner" role="alert">
            {skillHandoffError}
            <Button variant="ghost" size="sm" onClick={() => setSkillHandoffError(null)}>Dismiss</Button>
          </div>}
          {teamPicker ? <TeamPicker chatId={activeChatId} project={activeChat?.project_roots[0] ?? null} model={modelLabel(modelChoice)} onCreate={acceptTeam} onCancel={() => setTeamPicker(false)} /> : <>
          {specialists.length > 0 && <div className="ask-team-roster" aria-label="Chat specialists">
            <strong>Specialists</strong>
            {specialists.map((member) => <button type="button" key={member.slug} title={`Mention @${member.slug}`}
              className="ask-chip" disabled={ask.busy || !!activeChat?.archived}
              onClick={() => { setInput((previous) => `${previous.trimEnd()}${previous.trim() ? " " : ""}@${member.slug} `); inputRef.current?.focus(); }}>
              {member.name}
            </button>)}
          </div>}
          {ask.status && !productionReady && (
            <div className="ask-connect-banner">
              <span>No model gateway is ready.</span>
              <Button size="sm" onClick={() => onNavigate("models")}>Models &amp; Connections</Button>
            </div>
          )}

          <div className="ask-transcript-shell">
          <div className="ask-transcript" id="ask-transcript" ref={transcriptRef}
            role="region" aria-label="Conversation messages" tabIndex={0} onScroll={onTranscriptScroll}>
            {activeChat && <header className="ask-conversation-heading">
              <h2>{activeChat.title}</h2>
              <div className="ask-context-strip" aria-label="Working context">
                <span title={pinnedWorkspace}>{pinnedWorkspace ? <><FolderIcon width={14} height={14} aria-hidden /> Pinned project</> : workspaceAccess}</span>
                {pinnedWorkspace && <span>{workspaceAccess}</span>}
              </div>
            </header>}
            {(!messages?.length && !showLiveReply && !runVisible) && (
              <Welcome onSeed={(seed) => { handleInputChange(seed); inputRef.current?.focus(); }} />
            )}
            {messages?.map((m, i) => {
              const connectorReceipt = m.role === "user" ? connectorReceiptOf(m) : undefined;
              const skillSearch = connectorReceipt?.gateway === "skills-discovery" && connectorReceipt.outcome === "ok"
                ? readReceipt(connectorReceipt).discovery as SkillSearch | undefined : undefined;
              const turnEvidence = m.role === "user" ? evidenceOf(m) : [];
              const imageReceipt = m.role === "assistant" && m.gateway === "chat-images"
                ? evidenceOf(m).find((event) => event.tool === "chat_image" && event.kind === "tool" && event.outcome === "verified")
                : undefined;
              const imageData = imageReceipt ? readReceipt(imageReceipt) : undefined;
              const imagePath = imageData?.image && typeof imageData.image === "object" && "path" in imageData.image
                && typeof imageData.image.path === "string" ? imageData.image.path : undefined;
              const imageLabel = imageData?.source === "screenshot" ? "Ask PAIGE screenshot"
                : imageData?.source === "generated" ? "Generated image" : imagePath?.split("/").pop() ?? "Project PNG";
              const hasCommands = turnEvidence.some((event) => event.kind === "tool" && event.tool === "run_command");
              const hasPlan = turnEvidence.some(isWorkbenchPlan);
              const hasReport = turnEvidence.some((event) => event.tool === "create_word_report"
                && event.kind === "tool" && ["committed_verified", "target_verified"].includes(event.outcome));
              const selectedWorkflow = turnEvidence.find((event) => event.tool === "workflow_selected" && event.outcome === "selected");
              const selectedTemplate = selectedWorkflow && readReceipt(selectedWorkflow).template as WorkflowTemplate | undefined;
              const hasVisual = turnEvidence.some((event) => event.tool === "visual_before" && event.outcome === "verified");
              const hasComparison = turnEvidence.some((event) => event.tool === "visual_comparison" && event.outcome === "verified");
              const hasChanges = turnEvidence.some((event) => (event.kind === "tool" && event.tool === "git_diff") || hasWritePreview(event));
              return (
              <div className="ask-turn" key={`${m.turn_id ?? ""}-${i}`}>
              <div className={`ask-msg ask-msg-${m.role}`}>
                {m.role === "assistant" ? (
                  <div className="ask-msg-heading">
                    <div className="ask-msg-role">PAIGE</div>
                    <ReplyActions text={m.text} onActivity={m.turn_id && evidenceOf(m).some((event) => event.kind === "tool" || event.kind === "handoff")
                      ? (trigger) => showEvidence(m.turn_id ?? null, "Activity", trigger) : undefined} />
                  </div>
                ) : <div className="ask-msg-role">You</div>}
                {m.role === "assistant" && !connectorReceiptLabels[m.gateway ?? ""] && m.gateway !== "file-broker" ? (
                  <div className="ask-msg-text ask-reply">
                    <Markdown skipHtml disallowedElements={["a", "img"]} unwrapDisallowed>{m.text}</Markdown>
                  </div>
                ) : <div className="ask-msg-text">{m.text}</div>}
                {imageReceipt && activeChat && m.turn_id && (
                  <VerifiedPng chatId={activeChat.id} turnId={m.turn_id} label={imageLabel} alt={imageLabel}
                    onDisplay={() => { if (followLatest.current) transcriptRef.current?.scrollTo({ top: transcriptRef.current.scrollHeight }); }} />
                )}
                {m.role === "assistant" && m.model_label && (
                  <div className="ask-msg-model">{m.model_label}{turnRanNoTools(evidenceOf(m)) ? " · no tools executed" : ""}</div>
                )}
                {m.tool_events && m.tool_events.length > 0 && (
                  <div className="ask-msg-tools">
                    {m.tool_events.map((t, j) => (
                      <div key={j} className={`ask-tool ${t.ok ? "ok" : "err"}`}>
                        <span className="ask-tool-name">{t.tool}</span>
                        <span className="ask-tool-summary">{t.summary}</span>
                      </div>
                    ))}
                  </div>
                )}
                {!!m.turn_id && turnEvidence.length > 0 && (
                  <div className="ask-turn-links">
                    <button className="ask-whatpdid-toggle" onClick={() => showEvidence(m.turn_id ?? null)}>
                      What PAIGE did ({turnEvidence.length})
                    </button>
                    {selectedTemplate && workflowTemplates[selectedTemplate] && <button className="ask-whatpdid-toggle"
                      onClick={(event) => showEvidence(m.turn_id ?? null, "Activity", event.currentTarget)}>
                      {workflowTemplates[selectedTemplate].name} · selected from chat
                    </button>}
                    {hasPlan && <button className="ask-whatpdid-toggle" onClick={(event) => showEvidence(m.turn_id ?? null, "Plan", event.currentTarget)}>View plan</button>}
                    {hasChanges && <button className="ask-whatpdid-toggle" onClick={() => showEvidence(m.turn_id ?? null, "Changes")}>View changes</button>}
                    {hasReport && <button className="ask-whatpdid-toggle" onClick={(event) => showEvidence(m.turn_id ?? null, "Activity", event.currentTarget)}>View report</button>}
                    {hasVisual && <button className="ask-whatpdid-toggle" onClick={(event) => showEvidence(m.turn_id ?? null, "Activity", event.currentTarget)}>
                      {hasComparison ? "View comparison" : "View Visual QA"}
                    </button>}
                    {hasCommands && <button className="ask-whatpdid-toggle" onClick={() => showEvidence(m.turn_id ?? null, "Terminal")}>View command output</button>}
                  </div>
                )}
                {m.role === "user" && m.turn_id?.split("-").length === 4 && !runVisible && savedTask?.turn_id !== m.turn_id &&
                  !activeChat?.recorded_turn_ids.includes(m.turn_id) && (
                  <div className="ask-msg-model">Turn interrupted. Check the recorded activity before retrying.</div>
                )}
              </div>
              {connectorReceipt && (
                <button type="button" className="ask-inline-receipt"
                  onClick={() => showEvidence(m.turn_id ?? null)}>
                  <ActivityIcon width={16} height={16} aria-hidden />
                  <span>
                    <strong>{connectorReceiptLabels[connectorReceipt.gateway]}</strong>
                    <span>{[connectorReceipt.summary, receiptElapsed(connectorReceipt)].filter(Boolean).join(" · ")}</span>
                  </span>
                  <ChevronRightIcon width={14} height={14} aria-hidden />
                </button>
              )}
              {connectorReceipt?.gateway === "omniroute-web" && <SearchResults event={connectorReceipt} />}
              {skillSearch && <SkillDiscovery initial={skillSearch} onInstalled={onInstalled} onUse={onUseInstalled} />}
              {specialists.length > 0 && m.role === "user" && <TeamActivity events={teamActivityOf(m)}
                running={runVisible && !activeChat?.recorded_turn_ids.includes(m.turn_id ?? "")} />}
              </div>
              );
            })}
            {showLiveReply && (
              <div className="ask-msg ask-msg-assistant">
                <div className="ask-msg-role">PAIGE</div>
                {startingReply ? (
                  <div className="ask-starting" role="status" aria-label="PAIGE is starting a reply">
                    <span aria-hidden="true" /><span aria-hidden="true" /><span aria-hidden="true" />
                  </div>
                ) : null}
                {livePlan && <button type="button" className="ask-whatpdid-toggle"
                  onClick={(event) => showEvidence(livePlan.turn_id ?? null, "Plan", event.currentTarget)}>View plan</button>}
                {ask.toolEvents.length > 0 && (
                  <details className="ask-live-receipts">
                    <summary>What PAIGE did ({ask.toolEvents.length})</summary>
                    <div className="ask-msg-tools">
                      {ask.toolEvents.map((event, index) => (
                        <div key={index} className={`ask-tool ${event.ok ? "ok" : "err"}`}>
                          <span className="ask-tool-name">{event.tool}</span>
                          <span className="ask-tool-summary">{event.summary}</span>
                        </div>
                      ))}
                    </div>
                  </details>
                )}
              </div>
            )}
            {pendingApproval && <ApprovalCard event={pendingApproval}
              reviewed={approvalPreview !== null && approvalPreview.turn_id === pendingApproval.turn_id
                && readReceipt(approvalPreview).review_id === requestId}
              onReview={() => reviewApproval()} />}
            {ask.error && ask.error !== pauseExplanation && <div className="ask-error-banner">{ask.error}</div>}
            {chatError && (
              <div className="ask-error-banner" role="alert">
                {chatError}
                <Button variant="ghost" size="sm" onClick={() => void withChatAction(async () => { await refreshChats(); })}>
                  Reload conversations
                </Button>
              </div>
            )}
          </div>
          {awayFromLatest && (
            <button type="button" className="ask-jump-latest ask-round-control" aria-label="Jump to latest message"
              title="Jump to latest message" aria-controls="ask-transcript" onClick={jumpToLatest}>
              <ArrowUpIcon width={18} height={18} aria-hidden />
            </button>
          )}
          </div>

          <div className="ask-composer">
            {showLiveReply && <section className="ask-run-progress" aria-label="Task progress">
              <div className="ask-run-summary" role="status" aria-atomic="true">
                <span className="ask-run-indicator" data-tone={pendingApproval ? "warning" : "neutral"} aria-hidden="true" />
                <strong>{pendingApproval ? pendingApproval.outcome === "awaiting_confirmation" ? "Waiting for Windows confirmation" : "Waiting for your approval" : liveMessage}</strong>
              </div>
              {pendingApproval?.outcome === "awaiting_review"
                ? <Button size="sm" onClick={(event) => reviewApproval(event.currentTarget)}>{hasWritePreview(pendingApproval) ? "Review changes" : "Review action"}</Button>
                : <Button size="sm" variant="ghost" onClick={(event) => showEvidence(progressTurn ?? null, "Activity", event.currentTarget)}>View activity</Button>}
            </section>}
            {savedTask && !runVisible && (
              <section className="ask-run-progress" aria-label="Saved coding task">
                <div className="ask-run-summary" role="status" aria-atomic="true">
                  <span className="ask-run-indicator" data-tone="warning" aria-hidden="true" />
                  <div><strong>{savedTask.blocked ? "Task needs attention"
                    : needsTaskFolder ? "Choose a project to continue"
                    : needsSavedWorkspace ? "Workspace access needs attention"
                    : waitingForWorkspace ? "Ready to continue with this folder"
                    : savedTask.restart_waiting && !savedTask.restart_ready ? "Restart PAIGE to run the saved tests"
                    : savedTask.restart_ready ? "Saved tests are ready to run"
                    : "Paused at a saved checkpoint"}</strong>
                    {lastVerified?.step && <small>Last verified step · {lastVerified.step.title}</small>}
                    {(savedTask.blocked || pauseExplanation) && <small>{savedTask.blocked || pauseExplanation}</small>}
                  </div>
                </div>
                <div className="ask-progress-actions">
                {needsTaskFolder ? (
                  <Button size="sm" onClick={() => {
                    setProjectPicker(true);
                  }}>Choose project</Button>
                ) : needsSavedWorkspace ? (
                  <Button size="sm" onClick={restoreTaskWorkspace} disabled={ask.busy || chatPending || broker.busy || !!activeChat?.archived}>Restore workspace access</Button>
                ) : savedTask.restart_waiting && !savedTask.restart_ready ? null : (
                  <Button size="sm" onClick={() => resumeTask()} disabled={ask.busy || chatPending || !!savedTask.blocked || !!activeChat?.archived}>
                    {waitingForWorkspace ? "Continue with this folder" : "Resume task"}
                  </Button>
                )}
                {savedTask.can_retry_openrouter_free && !needsSavedWorkspace && !waitingForWorkspace && !savedTask.restart_waiting && !savedTask.blocked && (
                  <Button size="sm" onClick={() => resumeTask(true)} disabled={ask.busy || chatPending || !!activeChat?.archived}
                    title="Send this saved task’s context to OpenRouter Free for this task only">
                    Resume with OpenRouter Free
                  </Button>
                )}
                <details className="ask-progress-options"><summary>Task options</summary>
                  <Button size="sm" variant="ghost" onClick={(event) => showEvidence(savedTask.turn_id, lastVerified?.step?.tab ?? "Activity", event.currentTarget)}>View activity</Button>
                  <Button size="sm" variant="ghost" onClick={endTask} disabled={ask.busy || chatPending}>End task</Button>
                </details>
                </div>
              </section>
            )}
            {!showLiveReply && !savedTask && latestResult?.step && <section className="ask-run-progress" aria-label="Latest recorded result">
              <div className="ask-run-summary">
                <span className="ask-run-indicator" data-tone={latestResult.step.tone} aria-hidden="true" />
                <div><strong>{latestResult.step.title}</strong>{latestResult.step.detail && <small>{latestResult.step.detail}</small>}</div>
              </div>
              <Button size="sm" variant="ghost" onClick={(event) => showEvidence(latestResult.event.turn_id ?? null,
                latestResult.step?.tab ?? "Activity", event.currentTarget)}>
                {latestResult.step.tab === "Terminal" ? "View command receipt" : latestResult.step.tab === "Plan" ? "View plan" : "View changes"}
              </Button>
            </section>}
            {workflowChoices.length > 0 && <div className="ask-workflow-draft" aria-label="Choose a workflow">
              <div>
                <strong>This request fits more than one workflow. Which should I start?</strong>
                <div className="ask-word-actions">
                  {workflowChoices.map((template) => <Button key={template} size="sm" variant="secondary"
                    onClick={() => chooseWorkflow(template, true)}>{workflowTemplates[template].name}</Button>)}
                </div>
              </div>
              <Button size="sm" variant="ghost" onClick={() => {
                setWorkflowChoices([]);
                setSkipIntentText(input.trim());
                inputRef.current?.focus();
              }}>Keep chatting</Button>
            </div>}
            {workflowDraft && <div className="ask-workflow-draft">
              <div>
                <strong>{workflowTemplates[workflowDraft].name}</strong>
                {workflowIntentText ? <small role="status">Selected from your request</small>
                  : <small>{workflowTemplates[workflowDraft].prompt}</small>}
                {workflowIntentText && !visualProposalOnly && <small>Send to start this workflow in Build.</small>}
                {workflowDraft === "engineering_v1" && <small>Continues automatically for up to three 32-round batches on the selected route.</small>}
                {workflowDraft === "engineering_v1" && <label>
                  <input type="checkbox" checked={restartBeforeTests} disabled={ask.busy}
                    onChange={event => setRestartBeforeTests(event.target.checked)} /> Run saved offline Cargo tests after restart
                </label>}
                {workflowDraft === "visual_qa_v1" && <div className="ask-visual-fields">
                  <label>
                    <input type="checkbox" checked={captureWindow} disabled={ask.busy}
                      onChange={event => setCaptureWindow(event.target.checked)} /> Capture PAIGE’s window
                  </label>
                  {!captureWindow && <label>Before screenshot
                    <input ref={screenshotRef} type="text" value={beforePng} disabled={ask.busy} placeholder="screenshots/before.png"
                      onChange={event => setBeforePng(event.target.value)} />
                  </label>}
                  <label>CSS file
                    <input ref={stylesheetRef} type="text" value={workflowFile} disabled={ask.busy} placeholder="src/styles.css"
                      onChange={event => setWorkflowFile(event.target.value)} />
                  </label>
                  <small>{captureWindow ? "Windows will ask before capturing the visible PAIGE window. Use a stylesheet path relative to this chat’s worktree."
                    : (!beforePng.trim() && !workflowFile.trim() ? "Which PNG and stylesheet should I use?"
                    : !beforePng.trim() ? "Which PNG should I inspect?"
                    : !workflowFile.trim() ? "Which stylesheet should I review?" : "Send when ready.") + " Use paths relative to this chat’s worktree."}</small>
                  <small>{visualProposalOnly ? "PAIGE will stop at a proposed diff." : "PAIGE will request approval before making an edit."} Screenshot sharing requires approval.</small>
                </div>}
                {workflowDraft === "engineering_v1" && restartBeforeTests && <label>
                  Limit this run to one file (optional)
                  <input type="text" value={workflowFile} disabled={ask.busy} placeholder="src/path.rs"
                    onChange={event => setWorkflowFile(event.target.value)} />
                </label>}
              </div>
              <Button variant="ghost" size="sm" onClick={() => {
                setSkipIntentText(input.trim());
                setWorkflowDraft(null);
                setWorkflowIntentText(null);
                setBeforePng("");
                setCaptureWindow(false);
                setWorkflowFile("");
                inputRef.current?.focus();
              }} disabled={ask.busy}>{workflowIntentText ? "Keep chatting" : "Remove"}</Button>
            </div>}
            {/* Chips */}
            {projectPicker && <Card className="ask-palette ask-project-picker">
              <div className="ask-palette-title">Choose a project for this chat</div>
              {[...new Set([...(authorizedWorkspace ? [authorizedWorkspace] : []), ...savedFolders(), LAB_ROOT])].map((root) => (
                <button key={root} type="button" className="ask-palette-item ask-project-choice"
                  disabled={chatPending || ask.busy} onClick={() => openProject(root)}>
                  <FolderIcon width={16} height={16} aria-hidden />
                  <span><strong>{folderName(root)}</strong><small>{root}</small></span>
                </button>
              ))}
              <div className="ask-project-picker-actions">
                <Button variant="secondary" size="sm" disabled={chatPending || ask.busy} onClick={() => openProject()}>Browse…</Button>
                <Button variant="ghost" size="sm" disabled={chatPending} onClick={() => void withChatAction(async () => {
                  // No project: keep this conversation draft project-less. The
                  // conversation record is created when the first message is
                  // sent, never by merely opening one.
                  setProjectPicker(false);
                  requestAnimationFrame(() => inputRef.current?.focus());
                })}>No project</Button>
              </div>
            </Card>}
            <div className="ask-chips">
              {selectedSkill && !savedTask && <span className="ask-chip ask-skill-chip">
                <span id="ask-active-skill">{selectedSkill.name}</span>
                <button type="button" className="ask-chip-x" aria-label={`Remove ${selectedSkill.name} skill`}
                  disabled={ask.busy} onClick={() => { setSkillId(null); inputRef.current?.focus(); }}>×</button>
              </span>}
              <Button variant="ghost" size="sm" onClick={openTeamPicker}
                disabled={ask.busy || chatPending || !!activeChat?.archived}>{specialists.length ? "Manage specialists" : "Add specialists"}</Button>
              {!savedTask && attachments.map((a) => (
                <span key={a} className="ask-chip ask-chip-attach" title={`Attached: ${a}`}>
                  📎 {a}
                  <button className="ask-chip-x" onClick={() => setAttachments((p) => p.filter((x) => x !== a))}>×</button>
                </span>
              ))}
              {!savedTask && contextRefs.map((c) => (
                <span key={c} className="ask-chip ask-chip-ctx" title={`Context: ${c}`}>
                  @ {c}
                  <button className="ask-chip-x" onClick={() => setContextRefs((p) => p.filter((x) => x !== c))}>×</button>
                </span>
              ))}
            </div>

            {slashOpen && (
              <Card className="ask-palette">
                {SLASH_COMMANDS.map((c) => (
                  <button key={c.cmd} className="ask-palette-item" onClick={() => applySlash(c)}>
                    <code>{c.cmd}</code>
                    <span>{c.label}</span>
                  </button>
                ))}
              </Card>
            )}

            {atOpen && specialists.length > 0 && (
              <Card className="ask-palette">
                <div className="ask-palette-title">Mention a specialist</div>
                {specialists.filter((member) => `${member.slug} ${member.name}`.toLowerCase().includes(atQuery.toLowerCase())).map((member) => (
                  <button type="button" key={member.slug} className="ask-palette-item" onClick={() => {
                    setInput((previous) => previous.replace(/@[\w-]*$/, `@${member.slug} `));
                    setAtOpen(false);
                    inputRef.current?.focus();
                  }}>
                    <code>@{member.slug}</code> <span>{member.name}</span>
                  </button>
                ))}
              </Card>
            )}
            {atOpen && !savedTask && (
              <Card className="ask-palette">
                <div className="ask-palette-title">Reference a file (@)</div>
                {!brokerGranted && (
                  <div className="ask-palette-hint">Grant lab access to reference files (choose the project from the context line above).</div>
                )}
                {brokerGranted && filteredFiles.map((f) => (
                  <button key={f.rel_path} className="ask-palette-item" onClick={() => addContextRef(f.rel_path)}>
                    <code>@ {f.rel_path}</code>
                  </button>
                ))}
                {brokerGranted && filteredFiles.length === 0 && (
                  <div className="ask-palette-hint">No files match “{atQuery}”.</div>
                )}
              </Card>
            )}

            {attachOpen && !savedTask && (
              <Card className="ask-palette">
                <div className="ask-palette-title">Attach a file</div>
                <input className="ask-palette-search" placeholder="Filter files…" value={attachQuery} onChange={(e) => setAttachQuery(e.target.value)} autoFocus />
                {!brokerGranted && (
                  <div className="ask-palette-hint">Grant lab access to attach files (choose the project from the context line above).</div>
                )}
                {brokerGranted && filteredAttach.map((f) => (
                  <button key={f.rel_path} className="ask-palette-item" onClick={() => addAttachment(f.rel_path)}>
                    <code>📎 {f.rel_path}</code>
                    <span>{Math.ceil(f.size / 1024)} KB</span>
                  </button>
                ))}
              </Card>
            )}

            {composerMenu && <div className="pd-card ask-palette ask-composer-menu" id="ask-composer-options"
              onKeyDown={(event) => {
                if (event.key === "Escape") { event.stopPropagation(); setComposerMenu(null); menuButton.current?.focus(); }
              }}>
              {composerMenu === "options" && <>
                <button type="button" className="ask-palette-item" onClick={() => {
                  setComposerMenu(null); setAttachOpen(true);
                }}>Attach a file</button>
                <button type="button" className="ask-palette-item" onClick={() => {
                  setSkillQuery(""); setComposerMenu("skills");
                }}>Use a skill…</button>
                <button type="button" className="ask-palette-item" onClick={() => setComposerMenu("workflows")}>Run a workflow…</button>
              </>}
              {composerMenu === "skills" && <>
                <input className="ask-palette-search" type="search" placeholder="Search skills…" aria-label="Search skills"
                  autoFocus value={skillQuery} onChange={(event) => setSkillQuery(event.target.value)} />
                {matchingSkills.map((skill) => (
                  <button type="button" className="ask-palette-item ask-option-description" key={skill.id} onClick={() => {
                    setSkillId(skill.id); setComposerMenu(null); inputRef.current?.focus();
                  }}><strong>{skill.name}</strong><small>{skill.description}</small></button>
                ))}
                {!matchingSkills.length && <p className="ask-palette-hint">{enabledSkills.length ? "No matching skills." : "No skills enabled."}</p>}
                <button type="button" className="ask-palette-item" onClick={() => {
                  setComposerMenu(null);
                  window.dispatchEvent(new CustomEvent("paige-navigate", { detail: "settings" }));
                }}>Manage skills in Settings</button>
                <button type="button" className="ask-palette-item" onClick={() => {
                  setComposerMenu(null);
                  setSkillId(null);
                  setInput(skillQuery.trim() ? "Find a GitHub skill for " + skillQuery.trim() : "Find a GitHub skill for ");
                  inputRef.current?.focus();
                }}>Find skills on GitHub</button>
              </>}
              {composerMenu === "workflows" && (Object.keys(workflowTemplates) as WorkflowTemplate[]).map((id) => (
                <button type="button" className="ask-palette-item ask-option-description" key={id}
                  onClick={() => chooseWorkflow(id)}>
                  <strong>{workflowTemplates[id].name}</strong>
                  <small>{workflowTemplates[id].description}</small>
                </button>
              ))}
            </div>}
            <div className="ap-inputrow">
              <Composer
                input={input}
                inputRef={inputRef}
                inputPlaceholder={activeChat?.archived ? "Unarchive this chat to send a message"
                  : ask.busy ? "PAIGE is working..."
                  : savedTask ? "Check workspace access or look up a Linear issue while this task is paused"
                  : "Message Ask PAIGE, or @mention a specialist..."}
                inputAriaDescribedBy={selectedSkill && !savedTask ? "ask-active-skill" : undefined}
                inputDisabled={!historyLoaded || projectPicker || ask.busy || chatPending || !!activeChat?.archived}
                onInputChange={handleInputChange}
                onSend={send}
                busy={ask.busy}
                onCancel={ask.cancel}
                cancelLabel={mode === "build" ? "Pause" : "Cancel"}
                sendDisabled={!input.trim() || !historyLoaded || chatPending || !!runningChatId || projectPicker || !!activeChat?.archived}
                mode={composerMode}
                onSetMode={setMode}
                modeLocked={!!workflowDraft}
                workspaceLabel={(pinnedWorkspace ?? pendingWorkspace) ? "▦ " + folderName((pinnedWorkspace ?? pendingWorkspace) as string) : "◇ No project"}
                onWorkspaceOpen={() => {
                  if (pinnedWorkspace && !needsSavedWorkspace) {
                    if (!pinnedReady) openProject(pinnedWorkspace, true);
                    return;
                  }
                  if (needsSavedWorkspace && savedTask?.root) {
                    restoreTaskWorkspace();
                  }
                  else setProjectPicker((open) => !open);
                }}
                projectPickerOpen={projectPicker}
                accessLabel={scopeMode === "full" ? "Full access"
                  : scopeMode === "observe" ? "Observe — read only"
                  : scopeMode === "selected" ? "Selected access" : "Access unavailable"}
                accessFull={scopeMode === "full"}
                accessTitle={`Computer Access: ${scopeLabel}. ${scopeNote}`}
                menuTriggerNode={<button type="button" ref={menuButton} className="ap-btn icon" aria-label="More message options"
                  aria-expanded={composerMenu !== null} aria-controls="ask-composer-options"
                  disabled={!historyLoaded || ask.busy || chatPending || !!savedTask || !!activeChat?.archived}
                  onClick={() => { setComposerMenu((menu) => menu ? null : "options"); setAttachOpen(false); }}>＋</button>}
                modelNode={<div className="ask-composer-model"><ModelPicker status={ask.status} choice={modelChoice} onChange={changeModel}
                  disabled={ask.busy || !ask.status} /></div>}
                healthNode={<div className="ask-status-chip" title={gatewaySummary} aria-label={gatewaySummary}>
                  <span className={`ask-dot ${omnirouteHealth?.reachable && omnirouteHealth?.authenticated ? "on" : omnirouteHealth?.reachable ? "warn" : "off"}`} />
                  OmniRoute
                  <span className={`ask-dot ask-dot-second ${openrouterHealth?.authenticated ? "on" : "off"}`} />
                  OpenRouter
                </div>}
                routeLine={ask.busy && ask.selection?.selected ? <div className="ask-selected-line">
                  Route: {ask.selection.selected.id} · {ask.selection.selected.gateway}
                </div> : null}
              />
            </div>

          </div>
          </>}
        </main>

        {/* RIGHT — Project tools and receipts */}
          {compact && rightOpen && !teamPicker && <button className="ask-workbench-scrim" type="button" tabIndex={-1}
            aria-label="Dismiss Workbench" onClick={closeWorkbench} />}
          <aside className="ask-right" id="ask-right-panel" aria-label="Workbench" hidden={!rightOpen || teamPicker}>
            <div className="ask-workbench-header">
              <strong><ActivityIcon width={16} height={16} aria-hidden /> Workbench</strong>
              <button type="button" aria-label="Close Workbench" onClick={closeWorkbench}>×</button>
            </div>
            <div className="ask-workbench-tabs" role="tablist" aria-label="Workbench views"
              onKeyDown={(event) => {
                const index = workbenchTabs.indexOf(workbenchTab);
                const next = event.key === "ArrowRight" ? workbenchTabs[(index + 1) % workbenchTabs.length]
                  : event.key === "ArrowLeft" ? workbenchTabs[(index + workbenchTabs.length - 1) % workbenchTabs.length]
                  : event.key === "Home" ? workbenchTabs[0] : event.key === "End" ? workbenchTabs[workbenchTabs.length - 1] : null;
                if (next) {
                  event.preventDefault();
                  setWorkbenchTab(next);
                  document.getElementById("workbench-tab-" + next)?.focus();
                }
              }}>
              {workbenchTabs.map((tab) => <button type="button" key={tab} id={"workbench-tab-" + tab}
                role="tab" aria-selected={workbenchTab === tab} aria-controls={"workbench-view-" + tab}
                tabIndex={workbenchTab === tab ? 0 : -1} onClick={() => setWorkbenchTab(tab)}>{tab}</button>)}
            </div>
            <div className="ask-workbench-view" role="tabpanel" id="workbench-view-Plan" aria-labelledby="workbench-tab-Plan" hidden={workbenchTab !== "Plan"}>
              <WorkbenchPlan events={evidence} preparing={runVisible && ask.busy} />
            </div>
            <div className="ask-workbench-view" role="tabpanel" id="workbench-view-Files" aria-labelledby="workbench-tab-Files" hidden={workbenchTab !== "Files"}>
              {grantedWorkspace && rightOpen && workbenchTab === "Files"
                ? <Files key={grantedWorkspace} workspaceRoot={grantedWorkspace} />
                : <p className="ask-sidepanel-empty">Choose and authorize a project in this chat to browse its files.</p>}
            </div>
            <div className="ask-workbench-view" role="tabpanel" id="workbench-view-Terminal" aria-labelledby="workbench-tab-Terminal" hidden={workbenchTab !== "Terminal"}>
              {inspectTurn && evidence.some((event) => event.kind === "tool" && event.tool === "run_command")
                ? <><div className="ask-workbench-header"><strong>Recorded command output</strong>
                    <Button size="sm" variant="ghost" onClick={() => setInspectTurn(null)}>Open terminal</Button></div>
                    {evidence.filter((event) => event.kind === "tool" && event.tool === "run_command").map((event, index) => {
                      const output = readReceipt(event);
                      return <article className="ask-artifact" key={index}>
                        <strong>{event.summary}</strong>
                        {typeof output.exit_code === "number" && <span>Exit code {output.exit_code}</span>}
                        {typeof output.cwd === "string" && <span className="ask-wrap-path">{output.cwd}</span>}
                        <pre className="ask-artifact-details ask-terminal-output">{[output.stdout, output.stderr, output.error].filter((part) => typeof part === "string").join("\n") || event.summary}</pre>
                      </article>;
                    })}</>
                : null}
              {terminalRoots.map((root) => <div className="ask-workbench-terminal" key={root}
                hidden={root !== terminalWorkspace || (!!inspectTurn && evidence.some((event) => event.kind === "tool" && event.tool === "run_command"))}>
                <Terminal workspaceRoot={root} />
              </div>)}
              {!terminalWorkspace && !inspectTurn && <p className="ask-sidepanel-empty">{grantedWorkspace
                ? "This project is outside the terminal’s authorized roots."
                : "Choose and authorize a project in this chat to use the terminal."}</p>}
            </div>
            <div className="ask-workbench-view" role="tabpanel" id="workbench-view-Changes" aria-labelledby="workbench-tab-Changes" hidden={workbenchTab !== "Changes"}>
              {approvalPreview && <ProposedAction event={approvalPreview} />}
              {approvalPreview?.tool === "visual_inspection" && activeChat && approvalPreview.turn_id
                && <VisualScreenshot key={String(requestId)} chatId={activeChat.id} turnId={approvalPreview.turn_id} stage="before"
                  onDisplay={() => { if (typeof requestId === "string") setVisualPreviewId(requestId); }} />}
              {typeof requestId === "string" && pendingApproval?.outcome === "awaiting_review" && approvalPreview
                && approvalPreview.turn_id === pendingApproval.turn_id && readReceipt(approvalPreview).review_id === requestId
                && <div className="ask-confirm-action">
                  <Button type="button" size="sm" variant="primary" onClick={() => void openConfirmation()}
                    disabled={openingApproval === requestId || (pendingApproval.tool === "visual_inspection" && visualPreviewId !== requestId)}>
                    {openingApproval === requestId ? "Opening…" : "Open Windows confirmation"}
                  </Button>
                  {approvalError && <p className="ask-error-banner" role="alert">{approvalError}</p>}
                </div>}
              {!approvalPreview && !evidence.some((event) => event.tool === "git_diff" || hasWritePreview(event))
                && <p className="ask-sidepanel-empty">No changes recorded for this conversation.</p>}
              {evidence.filter((event) => event.kind === "tool" && event.tool === "git_diff").map((event, index) => (
                <Receipt event={event} key={index} />
              ))}
              {!approvalPreview && evidence.filter(hasWritePreview).map((event, index) => <ProposedAction event={event} key={index} />)}
            </div>
            <div className="ask-workbench-view" role="tabpanel" id="workbench-view-Activity" aria-labelledby="workbench-tab-Activity" hidden={workbenchTab !== "Activity"}>
            {inspectedWorkflow && activeChat && <WorkflowDetails chat={activeChat} run={inspectedWorkflow}
              running={ask.activeTurnId === inspectedWorkflow.turn_id} events={evidence}
              deciding={chatPending || ask.busy} onAccept={acceptWorkflowArtifact} onSaveWord={saveWordCopy} onCompareVisual={compareVisual} onRetryVisualTest={retryVisualTest} />}
            <div className="ask-right-head">Run timeline</div>
            {inspectTurn && <Button variant="secondary" size="sm" onClick={() => setInspectTurn(null)}>All conversation activity</Button>}
            {runVisible && !inspectTurn && <div className="ask-right-activity">
              <div className="ask-right-activity-title">Live activity</div>
              <div className="ask-right-activity-list">
                {ask.toolEvents.map((t, j) => (
                  <div className="ask-activity-event" key={j}>
                    <span className={`ask-activity-tool ask-activity-tool-${t.ok ? "ok" : "err"}`}>{t.tool}</span>
                    <span className="ask-activity-summary">{t.summary}</span>
                  </div>
                ))}
              </div>
            </div>}
            {timeline.length > 0 && <ol className="ask-run-timeline">
              {timeline.map(({ event, step }, index) => <li key={event.turn_id + ":" + event.timestamp + ":" + index}>
                <details>
                  <summary>
                    <span className="ask-run-indicator" data-tone={step.tone} aria-hidden="true" />
                    <span><strong>{step.title}</strong>{step.detail && <small>{step.detail}</small>}</span>
                    <ChevronRightIcon width={12} height={12} aria-hidden />
                  </summary>
                  <Receipt event={event} />
                </details>
</li>)}
            </ol>}
            {otherReceipts.length > 0 && <details className="ask-run-extra">
              <summary>{timeline.length ? "Other receipts" : "Recorded receipts"} ({otherReceipts.length})</summary>
              <div className="ask-right-artifacts">{otherReceipts.map((event, index) => <Receipt key={index} event={event} />)}</div>
            </details>}
            {evidence.length === 0 && <p className="ask-sidepanel-empty">No recorded activity for this conversation.</p>}
            </div>
          </aside>
      </div>
    </div>
  );
}

