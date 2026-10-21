import { createReadStream, createWriteStream } from 'node:fs';
import { access, mkdir, rename, rm, stat } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import path from 'node:path';
import { pipeline } from 'node:stream/promises';
import {
  dateFolder,
  selectCaptureDate,
  type DateOrganization,
  type MediaItem,
} from '@kairix-pimp/core';
import { resolveWithin } from './paths.js';

export interface IngestOptions {
  masterBin: string;
  sourcePath: string;
  relativeSourcePath?: string;
  dateOrganization: DateOrganization;
  now?: Date;
  embeddedDate?: Date;
  captureTimezone?: string;
  signal?: AbortSignal;
  onProgress?: (copiedBytes: number, totalBytes: number) => void;
}

export interface IngestResult {
  status: 'imported' | 'duplicate';
  destination: string;
  media: MediaItem;
}

export async function sha256(filePath: string): Promise<string> {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filePath))
    hash.update(chunk as Buffer);
  return hash.digest('hex');
}

export async function verifyMatchingChecksums(
  sourcePath: string,
  destinationPath: string,
): Promise<string> {
  const [sourceHash, destinationHash] = await Promise.all([
    sha256(sourcePath),
    sha256(destinationPath),
  ]);
  if (sourceHash !== destinationHash)
    throw new Error('Checksum verification failed');
  return sourceHash;
}

async function exists(filePath: string) {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

export async function ingestFile(
  options: IngestOptions,
): Promise<IngestResult> {
  const now = options.now ?? new Date();
  const source = await stat(options.sourcePath);
  if (!source.isFile()) throw new Error('Ingest source must be a file');
  const capture = selectCaptureDate({
    embedded: options.embeddedDate,
    filesystem: source.birthtime,
    timezone: options.captureTimezone,
  });
  const relative = path.join(
    'media',
    dateFolder(options.dateOrganization, now, capture.date),
    options.relativeSourcePath ?? path.basename(options.sourcePath),
  );
  const destination = resolveWithin(options.masterBin, relative);
  await mkdir(path.dirname(destination), { recursive: true });
  const sourceHash = await sha256(options.sourcePath);
  if (await exists(destination)) {
    if (
      (await stat(destination)).size === source.size &&
      (await sha256(destination)) === sourceHash
    ) {
      return {
        status: 'duplicate',
        destination,
        media: mediaRecord(relative, sourceHash, source.size, capture, now),
      };
    }
    throw new Error(`Destination collision: ${destination}`);
  }
  const temporary = `${destination}.pimp-part-${randomUUID()}`;
  let copied = 0;
  const input = createReadStream(options.sourcePath, {
    signal: options.signal,
  });
  input.on('data', (chunk) => {
    copied += Buffer.byteLength(chunk);
    options.onProgress?.(copied, source.size);
  });
  try {
    await pipeline(input, createWriteStream(temporary, { flags: 'wx' }), {
      signal: options.signal,
    });
    await verifyMatchingChecksums(options.sourcePath, temporary);
    await rename(temporary, destination);
    return {
      status: 'imported',
      destination,
      media: mediaRecord(relative, sourceHash, source.size, capture, now),
    };
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
}

function mediaRecord(
  relativePath: string,
  hash: string,
  sizeBytes: number,
  capture: ReturnType<typeof selectCaptureDate>,
  now: Date,
): MediaItem {
  return {
    id: `sha256:${hash}`,
    relativePath: relativePath.split(path.sep).join('/'),
    originalFilename: path.basename(relativePath),
    sha256: hash,
    captureDate: capture.date?.toISOString(),
    captureDateProvenance: capture.provenance,
    captureTimezone: capture.timezone,
    ingestDate: now.toISOString(),
    tags: [],
    projectIds: [],
    technical: { sizeBytes },
  };
}
