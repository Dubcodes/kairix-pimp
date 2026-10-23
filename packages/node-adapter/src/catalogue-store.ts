import { appendFile, mkdir, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import {
  SCHEMA_VERSION,
  rebuildCatalogue,
  type CatalogueEvent,
  type EventEnvelope,
  type MediaItem,
  type ProjectRecord,
} from '@kairix-pimp/core';
import { atomicWrite } from './atomic.js';

const eventDirectory = (masterBin: string) =>
  path.join(masterBin, '.pimp', 'events');

function migrateLegacyEnvelope(
  value: Record<string, unknown>,
  line: number,
): EventEnvelope {
  const deviceId = String(value.deviceId ?? 'legacy');
  const sequence = Number(value.sequence ?? line);
  const rawEvent = (value.event ?? value.payload) as Record<string, unknown>;
  const type = String(
    (value.event as Record<string, unknown> | undefined)?.type ?? value.type,
  );
  let event: CatalogueEvent;
  if (type === 'media.registered' || type === 'media.discovered') {
    event = {
      type: 'media.registered',
      media: (rawEvent.media ?? rawEvent) as MediaItem,
    };
  } else if (type === 'media.updated') {
    event = {
      type: 'media.legacy-snapshot-replaced',
      media: rawEvent as unknown as MediaItem,
    };
  } else if (type === 'media.tags-set') {
    event = {
      type: 'media.tags.replaced',
      mediaId: String(rawEvent.mediaId),
      values: (rawEvent.tags as string[]) ?? [],
    };
  } else if (type === 'media.projects-set') {
    event = {
      type: 'media.projects.replaced',
      mediaId: String(rawEvent.mediaId),
      values: (rawEvent.projectIds as string[]) ?? [],
    };
  } else if (type === 'project.upserted') {
    event = {
      type: 'project.upserted',
      project: (rawEvent.project ?? rawEvent) as ProjectRecord,
    };
  } else {
    throw new Error(`Unsupported legacy event type ${type}`);
  }
  return {
    schemaVersion: SCHEMA_VERSION,
    eventId: `v1-adapted-${String(value.eventId ?? `legacy-${deviceId}-${sequence}`)}`,
    deviceId,
    sequence,
    occurredAt: String(value.occurredAt ?? '1970-01-01T00:00:00Z'),
    observed: { [deviceId]: Math.max(0, sequence - 1) },
    event,
  };
}

export async function initializeCatalogue(masterBin: string): Promise<void> {
  await mkdir(eventDirectory(masterBin), { recursive: true });
  await mkdir(path.join(masterBin, '.pimp', 'previews'), { recursive: true });
  const schemaPath = path.join(masterBin, '.pimp', 'catalogue.json');
  try {
    await readFile(schemaPath);
  } catch {
    await atomicWrite(
      schemaPath,
      JSON.stringify(
        { schemaVersion: SCHEMA_VERSION, format: 'pimp-event-log' },
        null,
        2,
      ),
    );
  }
}

export async function appendCatalogueEvent(
  masterBin: string,
  envelope: EventEnvelope,
): Promise<void> {
  if (envelope.schemaVersion !== SCHEMA_VERSION)
    throw new Error(`Unsupported schema version ${envelope.schemaVersion}`);
  await initializeCatalogue(masterBin);
  const target = path.join(
    eventDirectory(masterBin),
    `${envelope.deviceId}.jsonl`,
  );
  await appendFile(target, `${JSON.stringify(envelope)}\n`, {
    encoding: 'utf8',
    flag: 'a',
  });
}

export function createEnvelope(
  deviceId: string,
  sequence: number,
  event: CatalogueEvent,
  occurredAt = new Date().toISOString(),
  observed: Record<string, number> = {},
): EventEnvelope {
  return {
    schemaVersion: SCHEMA_VERSION,
    eventId: crypto.randomUUID(),
    deviceId,
    sequence,
    occurredAt,
    observed: { ...observed, [deviceId]: sequence - 1 },
    event,
  };
}

export async function loadCatalogue(masterBin: string) {
  await initializeCatalogue(masterBin);
  const directoryEntries = await readdir(eventDirectory(masterBin));
  const files = directoryEntries.filter(
    (file) => file.endsWith('.jsonl') && !file.includes('.sync-conflict-'),
  );
  const events: EventEnvelope[] = [];
  const malformed: string[] = [];
  const unsupported: string[] = [];
  const conflictCopies = directoryEntries.filter((file) =>
    file.toLocaleLowerCase().includes('.sync-conflict-'),
  );
  for (const file of files) {
    const content = await readFile(
      path.join(eventDirectory(masterBin), file),
      'utf8',
    );
    const committed = content.endsWith('\n')
      ? content
      : content.slice(0, Math.max(0, content.lastIndexOf('\n') + 1));
    if (committed.length !== content.length)
      malformed.push(`${file}:truncated`);
    for (const [index, line] of committed.split(/\r?\n/).entries()) {
      if (!line.trim()) continue;
      try {
        const value = JSON.parse(line) as Record<string, unknown>;
        const version = Number(value.schemaVersion ?? 1);
        if (version > SCHEMA_VERSION) {
          unsupported.push(`${file}:${index + 1}`);
          continue;
        }
        const parsed =
          version === 1
            ? migrateLegacyEnvelope(value, index + 1)
            : (value as unknown as EventEnvelope);
        if (
          parsed.schemaVersion !== SCHEMA_VERSION ||
          !parsed.eventId ||
          !parsed.deviceId ||
          !Number.isSafeInteger(parsed.sequence) ||
          parsed.sequence < 1 ||
          !parsed.event
        )
          throw new Error('Invalid event envelope');
        parsed.observed ??= { [parsed.deviceId]: parsed.sequence - 1 };
        events.push(parsed);
      } catch {
        malformed.push(`${file}:${index + 1}`);
      }
    }
  }
  return {
    state: rebuildCatalogue(events),
    malformed,
    unsupported,
    conflictCopies,
  };
}
