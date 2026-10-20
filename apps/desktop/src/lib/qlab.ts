import type { QLabCountdown } from './types';

/**
 * `m:ss` or `h:mm:ss`, rounded up so `0:00` shows only as the cue ends.
 * Mirrors `QLabContent.formatTime` (Swift) and the `/qlab` page.
 */
export function formatTime(seconds: number): string {
  const total = Math.max(0, Math.ceil(seconds - 0.001));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

/** Share of the cue left, 0–100: the count-down bar drains to empty. */
export function remainingPercent(c: QLabCountdown): number {
  return c.duration > 0 ? Math.min(100, Math.max(0, (c.remaining / c.duration) * 100)) : 0;
}

/** The "Hide cues" box: numbers separated by commas or spaces. */
export function parseCueList(text: string): string[] {
  return [...new Set(text.split(/[\s,]+/).filter(Boolean))];
}

export function formatCueList(cues: string[]): string {
  return cues.join(', ');
}
