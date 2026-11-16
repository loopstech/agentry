/** Two-letter avatar initials for a project folder name.
 *  "api-server" → "AS", "projA" → "PA", "web" → "WB", "docs" → "DC". */
export function initials(project: string): string {
  const parts = project
    .replace(/([a-z])([A-Z0-9])/g, "$1 $2")
    .split(/[\s._-]+/)
    .filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  const word = parts[0] ?? "?";
  const consonant = word.slice(1).match(/[bcdfghjklmnpqrstvwxz]/i)?.[0];
  return (word[0] + (consonant ?? word[1] ?? "")).toUpperCase();
}

/** 42_000 → "0:42", 3_725_000 → "1:02:05". */
export function elapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
