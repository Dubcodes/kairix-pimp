import type { DateOrganization } from './types';

export interface FileDateEvidence {
  embedded?: Date;
  filesystem?: Date;
  timezone?: string;
}

export function selectCaptureDate(evidence: FileDateEvidence): {
  date?: Date;
  provenance: 'embedded' | 'filesystem' | 'unknown';
  timezone?: string;
} {
  const plausible = (date?: Date) =>
    date &&
    !Number.isNaN(date.getTime()) &&
    date.getUTCFullYear() >= 1990 &&
    date.getTime() <= Date.now() + 86_400_000;
  if (plausible(evidence.embedded))
    return {
      date: evidence.embedded,
      provenance: 'embedded',
      timezone: evidence.timezone,
    };
  if (plausible(evidence.filesystem))
    return { date: evidence.filesystem, provenance: 'filesystem' };
  return { provenance: 'unknown' };
}

export function dateFolder(
  mode: DateOrganization,
  now: Date,
  capture?: Date,
): string {
  const chosen = mode === 'original-file-dates' && capture ? capture : now;
  const year = chosen.getFullYear();
  const month = String(chosen.getMonth() + 1).padStart(2, '0');
  const day = String(chosen.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function inferFolderMetadata(name: string): {
  date?: string;
  suggestedTags: string[];
} {
  const match = name.match(/(?:^|\s)(\d{4}-\d{2}-\d{2})(?:\s|$)/);
  const validDate =
    match && !Number.isNaN(new Date(`${match[1]}T00:00:00`).getTime())
      ? match[1]
      : undefined;
  const remainder = name
    .replace(match?.[0] ?? '', ' ')
    .replace(/[_-]+/g, ' ')
    .trim();
  const suggestedTags = remainder
    ? remainder.split(/\s+/).filter((part) => part.length > 1)
    : [];
  return { date: validDate, suggestedTags };
}
