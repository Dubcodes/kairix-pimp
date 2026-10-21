import { appendFile, mkdir, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import {
  SCHEMA_VERSION,
  rebuildCatalogue,
  type CatalogueEvent,
  type EventEnvelope,
} from '@kairix-pimp/core';
import { atomicWrite } from './atomic.js';

const eventDirectory = (masterBin: string) =>
  path.join(masterBin, '.pimp', 'events');

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
): EventEnvelope {
  return {
    schemaVersion: SCHEMA_VERSION,
    eventId: crypto.randomUUID(),
    deviceId,
    sequence,
    occurredAt,
    event,
  };
}

export async function loadCatalogue(masterBin: string) {
  await initializeCatalogue(masterBin);
  const files = (await readdir(eventDirectory(masterBin))).filter(
    (file) => file.endsWith('.jsonl') && !file.includes('.sync-conflict-'),
  );
  const events: EventEnvelope[] = [];
  const malformed: string[] = [];
  for (const file of files) {
    const content = await readFile(
      path.join(eventDirectory(masterBin), file),
      'utf8',
    );
    for (const [index, line] of content.split(/\r?\n/).entries()) {
      if (!line.trim()) continue;
      try {
        const parsed = JSON.parse(line) as EventEnvelope;
        if (parsed.schemaVersion !== SCHEMA_VERSION)
          throw new Error('Unsupported schema');
        events.push(parsed);
      } catch {
        malformed.push(`${file}:${index + 1}`);
      }
    }
  }
  return { state: rebuildCatalogue(events), malformed };
}
