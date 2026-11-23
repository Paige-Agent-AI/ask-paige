import type { TeamMember, WorkflowTemplate } from "./types/chats";

interface IntentContext {
  /** Pass only the skills enabled in Settings and the specialists in this chat. */
  skills: ReadonlyArray<{ id: string; name: string }>;
  specialists: ReadonlyArray<Pick<TeamMember, "slug" | "name">>;
  attachments: readonly string[];
}

export type ChatIntent =
  | { kind: "workflow"; template: WorkflowTemplate; before?: string; css?: string; proposalOnly?: boolean; captureWindow?: boolean }
  | { kind: "choose_workflow"; templates: WorkflowTemplate[] }
  | { kind: "skill"; id: string }
  | { kind: "specialist"; slug: string }
  | { kind: "unavailable"; message: string };

// Match requests at their entry point. Explicit picks/commands and addressed
// messages are handled by their own entry points and have precedence.
function requestText(message: string): string {
  return message.trim()
    .replace(/^(?:(?:hey |okay |ok )?paige[,:]\s*)/i, "")
    .replace(/^(?:can|could|would|will) you\s+/i, "")
    .replace(/^please\s+/i, "")
    .replace(/\s+please[.!?]?$/i, "");
}

const recurrence = /\b(?:every (?:day|week|weekday|month|monday|tuesday|wednesday|thursday|friday|saturday|sunday)|on a (?:daily|weekly|monthly) basis|on a schedule)\b/i;
const paigeWindow = /\b(?:this screen|your own window|(?:this|the current|your) (?:PAIGE(?: Desktop)? |app )?window|(?:the )?PAIGE Desktop window)\b/i;
const visualSubject = /\b(?:screenshots?|screen ?captures?)\b|\.png\b/i;
const hasVisualSubject = (text: string) => visualSubject.test(text) || paigeWindow.test(text);
const visualCorrection = /\b(?:fix(?:es|ing)?|correct(?:ion|ions|ing)?|improv(?:e|ement|ements)|refine(?:ment)?)\b/i;
const visualAction = /^(?:capture|take a screenshot of|inspect|review|look at|check|analy[sz]e|evaluate|audit|compare|improve|fix|find|tell me|suggest|propose|show me)\b/i;
const externalChange = /\b(?:and|then|also)\s+(?:(?:please|go ahead and)\s+)?(?:deploy|publish|push|commit|merge|email|send|post|install|delete|schedule)\b/i;
const reviewOnly = /\b(?:review[- ]only|read[- ]only|only (?:inspect|review|propose|suggest)|keep (?:the )?(?:files?|source|worktree) unchanged|(?:without|do not|don't|don’t|never)\s+(?:(?:make|making) (?:any )?changes|apply(?:ing)?|fix(?:ing)?|edit(?:ing)?|chang(?:e|ing)|writ(?:e|ing)|run(?:ning)? (?:any |the )?(?:tests?|commands?)))\b/i;
const proposeCorrection = /\b(?:propose|suggest|recommend|show me|identify|describe)\b/i;

const workflows: ReadonlyArray<{ template: WorkflowTemplate; matches: (text: string) => boolean }> = [
  { template: "visual_qa_v1", matches: (text) => visualAction.test(text) && hasVisualSubject(text) && visualCorrection.test(text)
    && !/\b(?:without (?:uploading|sharing|sending|transmitting|disclosing)|(?:do not|don't|don’t|never) (?:upload|share|send|transmit|disclose)|keep (?:it|the (?:image|screenshot)) (?:local|offline|on (?:this|my|your) device))\b/i.test(text) },
  { template: "research_v1", matches: (text) => /^(?:research|investigate|look up|find|verify|compare|write|create|draft|prepare|turn)\b/i.test(text)
    && /\b(?:word (?:document|report|file)|docx)\b/i.test(text) },
  { template: "engineering_v1", matches: (text) => /^(?:fix|patch|implement|repair|resolve|make|change|address)\b/i.test(text)
    && /\b(?:bug|regression|issue|feature|worktree|tests?|code)\b/i.test(text)
    && !(visualAction.test(text) && hasVisualSubject(text)) },
];

function relativeFile(path: string, extension: "png" | "css"): boolean {
  return path.length <= 240 && path.toLowerCase().endsWith("." + extension)
    && !/[:\x00-\x1f\x7f]/.test(path)
    && path.split(/[\\/]/).every((part) => part && part !== "." && part !== ".." && !/[ .]$/.test(part));
}

function proposedFile(text: string, attachments: readonly string[], extension: "png" | "css") {
  const files = new Set<string>();
  // Quotes are needed for paths with spaces. Preserve spelling and case.
  const tokens = /“([^”\n]+)”|"([^"\n]+)"|\x60([^\x60\n]+)\x60|'([^'\n]+)'|[^\s"'“”\x60()[\]{}<>,!?;]+/g;
  for (const match of text.matchAll(tokens)) {
    const path = match[1] ?? match[2] ?? match[3] ?? match[4] ?? match[0].replace(/\.$/, "");
    if (relativeFile(path, extension)) files.add(path.replace(/\\/g, "/"));
  }
  for (const path of attachments) {
    if (relativeFile(path, extension)) files.add(path.replace(/\\/g, "/"));
  }
  return files.size === 1 ? files.values().next().value : undefined;
}

function matchingName(text: string, name: string) {
  return text.toLowerCase() === name.toLowerCase();
}

export function requestsVisualProposal(text: string): boolean {
  const request = requestText(text);
  const asksToApply = /^(?:fix|correct|improve|refine)\b/i.test(request)
    || /\b(?:and|then)\s+(?:apply|implement|make the (?:fix|change))\b/i.test(request);
  return reviewOnly.test(request) || !asksToApply;
}

export function resolveChatIntent(message: string, context: IntentContext): ChatIntent | null {
  const text = requestText(message);
  if (!text || /^(?:\/|["“'‘\x60]|if\b|what\b|how\b)/i.test(text)
    || /(?:^|\s)@[\w-]+\b(?![\w./-])/.test(text)) return null;

  if (/^(?:schedule\b|set (?:up )?(?:a|an|my) (?:reminder|automation|recurring job)\b|remind me\b)/i.test(text)
    || /^(?:every|each) (?:day|week|weekday|month|monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b/i.test(text)
    || (/^(?:research|monitor|check|send|publish|create|run|review|fetch|generate|capture|take a screenshot)\b/i.test(text)
      && !/\b(?:happens?|occurs?|appears?|shows? up)\b/i.test(text) && recurrence.test(text))) {
    return { kind: "unavailable", message: "Scheduled work isn’t available in PAIGE Desktop. You can ask me to run this once." };
  }
  if (externalChange.test(text) || /\b(?:unless|if)\b/i.test(text)) return null;
  const isReviewOnly = reviewOnly.test(text);
  if (isReviewOnly && !(hasVisualSubject(text) && proposeCorrection.test(text))) return null;

  const handoff = /^(?:ask|have|hand (?:this|it) to|delegate (?:this|it) to)\s+(?:the\s+)?(.+?)\s+(?:to|for)\s+\S/i.exec(text);
  if (handoff) {
    const matching = context.specialists.filter((member) => matchingName(handoff[1], member.name)
      || matchingName(handoff[1], member.slug));
    if (matching.length === 1) return { kind: "specialist", slug: matching[0].slug };
    return null;
  }

  const explicitSkill = /^(?:use|apply)\s+(?:the\s+)?(.+?)\s+skill(?:\s|[.!?,:]|$)/i.exec(text);
  if (explicitSkill) {
    const normalized = (value: string) => value.trim().toLowerCase().replace(/[ -]+/g, "-");
    const term = normalized(explicitSkill[1]);
    const matches = context.skills.filter(({ id, name }) => term === normalized(name) || term === normalized(id));
    if (matches.length === 1) return { kind: "skill", id: matches[0].id };
    return { kind: "unavailable", message: matches.length > 1
      ? "Several enabled skills share that name. Choose the repository you want under Settings → Skills."
      : "That skill isn’t enabled in this chat. Check Skills in Settings." };
  }

  const templates = workflows.filter(({ template, matches }) =>
    (!isReviewOnly || template === "visual_qa_v1") && matches(text)).map(({ template }) => template);
  if (templates.length > 1) return { kind: "choose_workflow", templates };
  if (templates[0] === "visual_qa_v1") {
    const before = proposedFile(message, context.attachments, "png");
    const captureWindow = paigeWindow.test(text) && !/\.png\b/i.test(text)
      && !context.attachments.some((file) => relativeFile(file, "png"))
      && !/\b(?:do not|don't|don’t|never|without)\s+(?:capture|capturing|take a screenshot)\b/i.test(text);
    return { kind: "workflow", template: "visual_qa_v1", proposalOnly: requestsVisualProposal(text),
      before, css: proposedFile(message, context.attachments, "css"), ...(captureWindow ? { captureWindow } : {}) };
  }
  if (templates.length === 1) return { kind: "workflow", template: templates[0] };

  const skill = context.skills.find(({ id }) => (
    id === "code-review" && /^review (?:this|the|my|our) (?:code|diff|patch|changes)\b/i.test(text)
    || id === "regression-testing" && /^(?:run|write|add) (?:(?:the|a|some) )?(?:focused )?regression tests?\b/i.test(text)
    || id === "source-verification" && /^verify (?:these|the|this) (?:claims?|sources?|citations?)\b/i.test(text)
  ));
  return skill ? { kind: "skill", id: skill.id } : null;
}
