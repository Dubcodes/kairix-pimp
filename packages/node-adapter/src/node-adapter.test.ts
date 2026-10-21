import {
  mkdtemp,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
  appendFile,
} from 'node:fs/promises';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { afterEach, describe, expect, it } from 'vitest';
import {
  appendCatalogueEvent,
  createEnvelope,
  generateThumbnail,
  ingestFile,
  loadCatalogue,
  resolveWithin,
  saveDeviceSettings,
  loadDeviceSettings,
  saveSharedSettings,
  loadSharedSettings,
  probeMedia,
  writeProjectManifest,
  verifyMatchingChecksums,
} from './index.js';

const exec = promisify(execFile);

const roots: string[] = [];
async function fixture() {
  const fixtureRoot = path.resolve('.test-tmp');
  await mkdir(fixtureRoot, { recursive: true });
  const root = await mkdtemp(path.join(fixtureRoot, 'pimp-test-'));
  roots.push(root);
  const source = path.join(root, 'source');
  const master = path.join(root, 'master');
  const projects = path.join(root, 'projects');
  await Promise.all([mkdir(source), mkdir(master), mkdir(projects)]);
  return { root, source, master, projects };
}
afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe('safe ingest', () => {
  it('copies Unicode names, independently verifies and preserves source', async () => {
    const { source, master } = await fixture();
    const input = path.join(source, 'clip ü 01.mp4');
    await writeFile(input, 'synthetic-camera-bytes');
    const result = await ingestFile({
      masterBin: master,
      sourcePath: input,
      dateOrganization: 'today',
      now: new Date(2026, 9, 22),
    });
    expect(result.status).toBe('imported');
    expect(await readFile(input, 'utf8')).toBe('synthetic-camera-bytes');
    expect(await readFile(result.destination, 'utf8')).toBe(
      'synthetic-camera-bytes',
    );
    expect(result.media.relativePath).toContain('2026-10-22');
  });
  it('detects a duplicate import by content identity', async () => {
    const { source, master } = await fixture();
    const input = path.join(source, 'same.mov');
    await writeFile(input, 'same');
    await ingestFile({
      masterBin: master,
      sourcePath: input,
      dateOrganization: 'today',
      now: new Date(2026, 9, 22),
    });
    const again = await ingestFile({
      masterBin: master,
      sourcePath: input,
      dateOrganization: 'today',
      now: new Date(2026, 9, 22),
    });
    expect(again.status).toBe('duplicate');
  });
  it('refuses same names with different content', async () => {
    const { source, master } = await fixture();
    const first = path.join(source, 'same.mov');
    await writeFile(first, 'one');
    await ingestFile({
      masterBin: master,
      sourcePath: first,
      dateOrganization: 'today',
      now: new Date(2026, 9, 22),
    });
    await writeFile(first, 'two');
    await expect(
      ingestFile({
        masterBin: master,
        sourcePath: first,
        dateOrganization: 'today',
        now: new Date(2026, 9, 22),
      }),
    ).rejects.toThrow('collision');
  });
  it('cleans temporary data after cancellation', async () => {
    const { source, master } = await fixture();
    const input = path.join(source, 'large.mov');
    await writeFile(input, Buffer.alloc(2_000_000, 7));
    const controller = new AbortController();
    await expect(
      ingestFile({
        masterBin: master,
        sourcePath: input,
        dateOrganization: 'today',
        signal: controller.signal,
        onProgress: () => controller.abort(),
      }),
    ).rejects.toThrow();
    const media = path.join(master, 'media');
    const entries = await stat(media)
      .then(() =>
        import('node:fs/promises').then((fs) =>
          fs.readdir(media, { recursive: true }),
        ),
      )
      .catch(() => []);
    expect(entries.some((entry) => String(entry).includes('.pimp-part-'))).toBe(
      false,
    );
  });
  it('reports a source removed before ingest', async () => {
    const { source, master } = await fixture();
    await expect(
      ingestFile({
        masterBin: master,
        sourcePath: path.join(source, 'missing.mp4'),
        dateOrganization: 'today',
      }),
    ).rejects.toThrow();
  });
  it('detects a corrupted destination during independent verification', async () => {
    const { source, master } = await fixture();
    const input = path.join(source, 'source.mov');
    const corrupt = path.join(master, 'corrupt.mov');
    await writeFile(input, 'original');
    await writeFile(corrupt, 'corrupted');
    await expect(verifyMatchingChecksums(input, corrupt)).rejects.toThrow(
      'Checksum verification failed',
    );
  });
});

describe('metadata durability', () => {
  it('rebuilds from append-only events and reports malformed records', async () => {
    const { master } = await fixture();
    const media = {
      id: 'x',
      relativePath: 'media/x.mp4',
      originalFilename: 'x.mp4',
      sha256: 'x',
      captureDateProvenance: 'unknown' as const,
      ingestDate: new Date().toISOString(),
      tags: [],
      projectIds: [],
      technical: { sizeBytes: 1 },
    };
    await appendCatalogueEvent(
      master,
      createEnvelope('device-a', 1, { type: 'media.registered', media }),
    );
    await appendFile(
      path.join(master, '.pimp', 'events', 'device-a.jsonl'),
      '{bad json}\n',
    );
    const loaded = await loadCatalogue(master);
    expect(loaded.state.media.x).toBeDefined();
    expect(loaded.malformed).toHaveLength(1);
  });
  it('keeps device paths local and shared preference portable', async () => {
    const { root, master } = await fixture();
    const local = {
      deviceId: 'a',
      masterBinPath: master,
      workingProjectsPath: 'C:/Work',
    };
    await saveDeviceSettings(root, local);
    await saveSharedSettings(master, {
      schemaVersion: 1,
      ingestDateOrganization: 'original-file-dates',
      updatedAt: new Date().toISOString(),
      updatedBy: 'a',
    });
    expect(await loadDeviceSettings(root)).toEqual(local);
    expect((await loadSharedSettings(master))?.ingestDateOrganization).toBe(
      'original-file-dates',
    );
  });
  it('writes a portable project manifest without replacing existing assets', async () => {
    const { projects } = await fixture();
    const existing = path.join(projects, 'vlog', 'graphics', 'title.png');
    await mkdir(path.dirname(existing), { recursive: true });
    await writeFile(existing, 'asset');
    const project = {
      id: 'p1',
      name: 'Vlog',
      workingDirectory: 'vlog',
      mediaIds: ['m1'],
      createdAt: new Date().toISOString(),
    };
    const media = {
      id: 'm1',
      relativePath: 'media/a.mp4',
      originalFilename: 'a.mp4',
      sha256: 'a',
      captureDateProvenance: 'unknown' as const,
      ingestDate: new Date().toISOString(),
      tags: ['A'],
      projectIds: ['p1'],
      technical: { sizeBytes: 1 },
    };
    const manifest = await writeProjectManifest(projects, project, [media]);
    expect(
      JSON.parse(await readFile(manifest, 'utf8')).project.workingDirectory,
    ).toBe('.');
    expect(await readFile(existing, 'utf8')).toBe('asset');
  });
  it('preserves folder identity through a move event', async () => {
    const { master } = await fixture();
    await appendCatalogueEvent(
      master,
      createEnvelope('device-a', 1, {
        type: 'folder.upserted',
        folder: {
          id: 'folder-1',
          relativePath: 'media/old',
          displayName: 'Old',
          tags: [],
        },
      }),
    );
    await appendCatalogueEvent(
      master,
      createEnvelope('device-a', 2, {
        type: 'folder.moved',
        folderId: 'folder-1',
        relativePath: 'media/new',
      }),
    );
    expect(
      (await loadCatalogue(master)).state.folders['folder-1'].relativePath,
    ).toBe('media/new');
  });
});

describe('FFmpeg integration', () => {
  it.each([
    ['H.264', 'libx264', 'h264'],
    ['H.265', 'libx265', 'hevc'],
  ])(
    'probes synthetic %s video and generates a thumbnail',
    async (_label, encoder, codec) => {
      const { root } = await fixture();
      const video = path.join(root, `${codec}.mp4`);
      const thumbnail = path.join(root, `${codec}.jpg`);
      await exec('ffmpeg', [
        '-v',
        'error',
        '-f',
        'lavfi',
        '-i',
        'color=c=blue:s=320x180:d=0.2',
        '-c:v',
        encoder,
        '-pix_fmt',
        'yuv420p',
        video,
      ]);
      const metadata = await probeMedia(video);
      expect(metadata.codec).toBe(codec);
      expect(metadata.width).toBe(320);
      await generateThumbnail(video, thumbnail, 0);
      expect((await stat(thumbnail)).size).toBeGreaterThan(0);
    },
    30_000,
  );
});

describe('path safety', () => {
  it.each([
    '../escape',
    '..\\escape',
    'C:\\outside',
    '/outside',
    '\\\\server\\share',
  ])('rejects out-of-root path %s', (candidate) =>
    expect(() => resolveWithin(path.resolve('master'), candidate)).toThrow(),
  );
});
