export function count(value: number | null): string {
  if (value === null) return "—";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 10_000) return `${Math.round(value / 1000)}K`;
  if (value >= 1_000) return `${(value / 1000).toFixed(1)}K`;
  return String(value);
}

export function bytes(value: number): string {
  if (value >= 1_048_576) return `${(value / 1_048_576).toFixed(1)} MB`;
  if (value >= 1024) return `${Math.round(value / 1024)} KB`;
  return `${value} B`;
}

/** "2026-09-04T09:12:00Z" → "4 Sep, 09:12". Falsy input reads as never. */
export function when(iso: string | null): string {
  if (!iso) return "never";

  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;

  return at.toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * "2026-09-01T00:00:00Z" → "1 Sep 2026", formatted in UTC.
 *
 * Some sources publish dates without times, stored as UTC midnight; rendering
 * those in local time walks the date backwards for anyone west of Greenwich.
 */
export function day(iso: string | null): string {
  if (!iso) return "an unknown date";

  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;

  return at.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
}

/** The two initials an addon shows when its source publishes no icon. */
export function initials(name: string): string {
  const words = name.split(/[\s!_-]+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}
