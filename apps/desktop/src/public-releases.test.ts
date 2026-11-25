import { expect, it, vi } from 'vitest';
import { checkPublicReleases, comparePublicVersions } from './public-releases';

const asset = (name: string, tag = 'v0.2.0') => ({
  name,
  size: 100,
  browser_download_url: `https://github.com/Dubcodes/kairix-pimp/releases/download/${tag}/${name}`,
});

it('compares integer components and legacy build suffixes', () => {
  expect(comparePublicVersions('1.22.0', '1.3.0')).toBeGreaterThan(0);
  expect(comparePublicVersions('0.1.1-5', '0.1.1-4')).toBeGreaterThan(0);
  expect(comparePublicVersions('0.2.0', '0.1.1-5')).toBeGreaterThan(0);
});

it('accepts only complete permanent public installer releases', async () => {
  const request = vi.fn(
    async () =>
      new Response(
        JSON.stringify([
          {
            tag_name: 'v0.2.0',
            html_url:
              'https://github.com/Dubcodes/kairix-pimp/releases/tag/v0.2.0',
            published_at: '2026-11-24T00:00:00Z',
            body: 'Release notes',
            draft: false,
            prerelease: true,
            assets: [
              asset('Kairix-PIMP-Light-0.2.0-Windows-x64-Setup-Unsigned.exe'),
              asset('Kairix-PIMP-Light-0.2.0-Windows-x64-Unsigned.msi'),
              asset('SHA256SUMS.txt'),
              asset('BUILD-INFO.txt'),
            ],
          },
        ]),
        { status: 200 },
      ),
  );
  await expect(
    checkPublicReleases('0.1.1-5', 'Development', request),
  ).resolves.toMatchObject({
    version: '0.2.0',
    classification: 'Development',
    newerThanInstalled: true,
    versionComparison: 'newer',
  });
});

it('distinguishes equal SemVer because it does not prove equal executable bytes', async () => {
  const request = vi.fn(
    async () =>
      new Response(
        JSON.stringify([
          {
            tag_name: 'v0.3.0',
            html_url:
              'https://github.com/Dubcodes/kairix-pimp/releases/tag/v0.3.0',
            published_at: '2026-11-24T00:00:00Z',
            body: 'Release notes',
            draft: false,
            prerelease: true,
            assets: [
              asset(
                'Kairix-PIMP-Light-0.3.0-Windows-x64-Setup-Unsigned.exe',
                'v0.3.0',
              ),
              asset('SHA256SUMS.txt', 'v0.3.0'),
              asset('BUILD-INFO.txt', 'v0.3.0'),
            ],
          },
        ]),
        { status: 200 },
      ),
  );
  await expect(
    checkPublicReleases('0.3.0', 'Development', request),
  ).resolves.toMatchObject({
    newerThanInstalled: false,
    versionComparison: 'same',
  });
});

it('rejects incomplete, draft and untrusted asset URLs', async () => {
  const request = vi.fn(
    async () =>
      new Response(
        JSON.stringify([
          {
            tag_name: 'v9.0.0',
            html_url:
              'https://github.com/Dubcodes/kairix-pimp/releases/tag/v9.0.0',
            published_at: '2026-11-24T00:00:00Z',
            body: 'Untrusted',
            draft: false,
            prerelease: false,
            assets: [
              {
                ...asset(
                  'Kairix-PIMP-Light-9.0.0-Windows-x64-Setup-Unsigned.exe',
                  'v9.0.0',
                ),
                browser_download_url: 'https://example.com/installer.exe',
              },
              asset('SHA256SUMS.txt', 'v9.0.0'),
              asset('BUILD-INFO.txt', 'v9.0.0'),
            ],
          },
        ]),
        { status: 200 },
      ),
  );
  await expect(
    checkPublicReleases('0.2.0', 'Development', request),
  ).resolves.toBeNull();
});
