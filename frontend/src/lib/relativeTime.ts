// Compact relative-time label ("just now", "5m", "3h", "2d", …) for the
// timeline feed. Falls back to an absolute short date past a week so old rows
// stay meaningful. Kept separate from `format.ts` (the frozen shared helpers).

const shortDate = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

const shortDateYear = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  year: "numeric",
});

/** e.g. `just now`, `12m`, `5h`, `3d`, `Apr 7`, `Apr 7, 2023`. */
export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  const t = d.getTime();
  if (Number.isNaN(t)) return "";

  const now = Date.now();
  const diffSec = Math.round((now - t) / 1000);

  if (diffSec < 45) return "just now";
  if (diffSec < 3600) return `${Math.max(1, Math.round(diffSec / 60))}m`;
  if (diffSec < 86_400) return `${Math.round(diffSec / 3600)}h`;
  if (diffSec < 7 * 86_400) return `${Math.round(diffSec / 86_400)}d`;

  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return (sameYear ? shortDate : shortDateYear).format(d);
}
