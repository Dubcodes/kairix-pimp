import path from 'node:path';
import { mkdir, readFile } from 'node:fs/promises';
import {
  SCHEMA_VERSION,
  type MediaItem,
  type ProjectRecord,
} from '@kairix-pimp/core';
import { atomicWrite } from './atomic.js';
import { resolveWithin } from './paths.js';

export async function writeProjectManifest(
  workingProjectsRoot: string,
  project: ProjectRecord,
  media: MediaItem[],
): Promise<string> {
  const directory = resolveWithin(
    workingProjectsRoot,
    project.workingDirectory,
  );
  await mkdir(directory, { recursive: true });
  const target = path.join(directory, '.pimp-project.json');
  try {
    const existing = JSON.parse(await readFile(target, 'utf8')) as {
      schemaVersion?: number;
      id?: string;
      project?: { id?: string };
    };
    if (
      (existing.schemaVersion ?? 1) > SCHEMA_VERSION ||
      (existing.id ?? existing.project?.id) !== project.id
    )
      throw new Error(
        'Existing .pimp-project.json is incompatible and was not overwritten',
      );
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  await atomicWrite(
    target,
    JSON.stringify(
      {
        schemaVersion: SCHEMA_VERSION,
        project: { ...project, workingDirectory: '.' },
        media: media.map(({ id, relativePath, sha256, tags }) => ({
          id,
          relativePath,
          sha256,
          tags,
        })),
      },
      null,
      2,
    ),
  );
  return target;
}
