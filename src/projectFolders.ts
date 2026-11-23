const SAVED_PROJECTS = "paige.savedProjectFolders";
export const LAB_ROOT = "C:\\Users\\tonig\\paige-desktop-lab";

export function savedFolders(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(SAVED_PROJECTS) ?? "[]");
    if (Array.isArray(value)) return value.filter((path): path is string => typeof path === "string");
  } catch { /* An unreadable preference grants no file access. */ }
  return [];
}

export function rememberFolder(root: string): string[] {
  const folders = savedFolders();
  if ([...folders, LAB_ROOT].some((saved) => saved.toLowerCase() === root.toLowerCase())) return folders;
  const next = [...folders, root];
  localStorage.setItem(SAVED_PROJECTS, JSON.stringify(next));
  return next;
}

export function folderName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
