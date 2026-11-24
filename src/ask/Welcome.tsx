import { PaigeCommandMark } from "../components/brand/PaigeCommandMark";

/**
 * The Stage 2A welcome — a clean, inviting entry with no mandatory project
 * selection. Every suggestion performs one genuine, supported action: it
 * seeds the composer with the owner's intent and hands focus to the input.
 * Adaptive headlines per conversation type arrive with Stage 2C; nothing here
 * pretends they exist yet.
 */
const suggestions: { title: string; hint: string; seed: string }[] = [
  { title: "Build something", hint: "Code, fix, ship", seed: "Help me build " },
  { title: "Research something", hint: "Read, compare, brief", seed: "Research " },
  { title: "Manage my computer", hint: "Files, folders, tools", seed: "Help me organize " },
  { title: "Work on my business", hint: "Ops and decisions", seed: "Help me with " },
];

export function Welcome({ onSeed }: { onSeed: (text: string) => void }) {
  return (
    <div className="ap-welcome">
      <PaigeCommandMark className="ap-welcome-mark" label="Ask PAIGE" />
      <h2>What are we <b>working on</b> today?</h2>
      <p>Start with your intent. A project is optional — pick one from the composer
        whenever the work calls for it.</p>
      <div className="ap-welcome-sugg">
        {suggestions.map(({ title, hint, seed }) => (
          <button type="button" key={title} onClick={() => onSeed(seed)}>
            {title}<span>{hint}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
