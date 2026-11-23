import skills from "./skills.json";

const key = "paige.skills.enabled";
export const availableSkills = skills;

export interface InstalledSkill {
  id: string;
  name: string;
  description: string;
  source: string;
  repository: string;
  commit: string;
  manifest: string;
}

export type SkillInfo = Pick<(typeof skills)[number], "id" | "name" | "description" | "design">
  & Partial<Pick<InstalledSkill, "source" | "repository" | "commit" | "manifest">>;

export function readEnabledSkills(): string[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "null");
    if (Array.isArray(saved)) {
      return skills.filter((skill) => saved.includes(skill.id) || skill.design).map((skill) => skill.id);
    }
    if (saved && typeof saved === "object" && "enabled" in saved && Array.isArray(saved.enabled)) {
      const ids = new Set(saved.enabled);
      return [...skills.filter((skill) => ids.has(skill.id)).map((skill) => skill.id),
        ...[...ids].filter((id): id is string => typeof id === "string" && /^github-[0-9a-f]{24}$/.test(id))];
    }
  } catch {
    // Fall back to the bundled defaults when settings cannot be read.
  }
  return skills.map((skill) => skill.id);
}

export function saveEnabledSkills(ids: string[]) {
  localStorage.setItem(key, JSON.stringify({ enabled: ids }));
}
