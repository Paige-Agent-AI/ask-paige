import { useId, useRef, useState } from "react";
import type { AskStatus, ModelCandidate } from "../types/ask";

const gatewayNames: Record<string, string> = {
  omniroute: "OmniRoute",
  "openrouter-direct": "OpenRouter",
  "test-harness": "Test gateway",
};

const routeLabels: Record<string, string> = {
  "omniroute::auto/best-free": "OmniRoute · Free Models Auto",
  "openrouter-direct::openrouter/auto": "OpenRouter · Auto",
  "openrouter-direct::openrouter/free": "OpenRouter · Free",
};

function modelKey(model: ModelCandidate) {
  return `${model.gateway}::${model.id}`;
}

export function modelLabel(choice: string | null) {
  if (!choice || choice === "auto") return "PAIGE Auto";
  if (Object.prototype.hasOwnProperty.call(routeLabels, choice)) return routeLabels[choice];
  return choice.includes("::") ? choice.split("::", 2)[1] : choice;
}

interface ModelPickerProps {
  status: AskStatus | null;
  choice: string | null;
  disabled: boolean;
  onChange: (value: string) => Promise<void>;
}

export function ModelPicker({ status, choice, disabled, onChange }: ModelPickerProps) {
  const panelId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const options = useRef<HTMLDivElement>(null);
  const ready = new Set(status?.gateways.filter((gateway) => gateway.reachable && gateway.authenticated)
    .map((gateway) => gateway.gateway));
  const catalog = new Map(status?.catalog.filter((model) => ready.has(model.gateway))
    .map((model) => [modelKey(model), model]));
  const gateway = choice?.includes("::") ? choice.split("::", 1)[0] : null;
  const label = gateway && choice && !(choice in routeLabels)
    ? `${modelLabel(choice)} · ${gatewayNames[gateway] ?? gateway}`
    : modelLabel(choice);
  const terms = query.trim().toLowerCase().split(/\s+/);
  const matchesQuery = (value: string) => terms.every((term) => value.toLowerCase().includes(term));
  const matches = [...catalog].filter(([key, model]) =>
    matchesQuery(`${modelLabel(key)} ${model.id} ${gatewayNames[model.gateway] ?? model.gateway}`));
  const showPaigeAuto = matchesQuery("PAIGE Auto");
  const autoChoices = matches.filter(([key]) => key in routeLabels);
  const models = matches.filter(([key]) => !(key in routeLabels));

  function close() {
    setOpen(false);
    setQuery("");
  }

  function pick(key: string) {
    close();
    trigger.current?.focus();
    void onChange(key);
  }

  return (
    <div className="ask-model"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) close();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          event.stopPropagation();
          close();
          trigger.current?.focus();
        }
      }}>
      <span className="ask-model-label" aria-hidden="true">Model</span>
      <button ref={trigger} type="button" className="ask-model-trigger" disabled={disabled}
        aria-expanded={open && !disabled} aria-controls={open && !disabled ? panelId : undefined}
        aria-label={`Model: ${label}`} title={label}
        onClick={() => open ? close() : setOpen(true)}>
        <span>{label}</span>
        <span aria-hidden="true">⌄</span>
      </button>
      {open && !disabled && (
        <div className="ask-model-panel" id={panelId} role="region" aria-label="Choose a model">
          <input className="ask-model-search" type="search" aria-label="Search models"
            placeholder="Search models or providers" autoFocus value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                options.current?.querySelector("button")?.focus();
              }
            }} />
          <div className="ask-model-options" ref={options}>
            {showPaigeAuto && (
              <button type="button" aria-pressed={!choice || choice === "auto"} onClick={() => pick("auto")}>
                <strong>PAIGE Auto</strong>
                <small>OmniRoute preferred · OpenRouter alternate</small>
              </button>
            )}
            {autoChoices.map(([key]) => (
              <button key={key} type="button" aria-pressed={choice === key} onClick={() => pick(key)}>
                <strong>{routeLabels[key]}</strong>
                <small>{key === "openrouter-direct::openrouter/auto" ? "May use paid models"
                  : key === "openrouter-direct::openrouter/free" ? "Free model pool" : "OmniRoute routes within your configured pool"}</small>
              </button>
            ))}
            {models.map(([key, model]) => (
              <button key={key} type="button" aria-pressed={choice === key} onClick={() => pick(key)}>
                <strong>{model.id}</strong>
                <small>{gatewayNames[model.gateway] ?? model.gateway}</small>
              </button>
            ))}
            {!showPaigeAuto && matches.length === 0 && <p role="status">No models match your search.</p>}
          </div>
        </div>
      )}
    </div>
  );
}
