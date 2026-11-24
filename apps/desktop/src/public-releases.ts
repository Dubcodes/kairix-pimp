export interface PublicRelease {
  version: string;
  classification: 'Development' | 'Stable';
  publishedAt: string;
  notes: string;
  releaseUrl: string;
  installerUrl: string;
  installerSizeBytes: number;
  checksumUrl: string;
  buildInfoUrl: string;
  newerThanInstalled: boolean;
}

interface GitHubAsset {
  name?: unknown;
  size?: unknown;
  browser_download_url?: unknown;
}

interface GitHubRelease {
  tag_name?: unknown;
  html_url?: unknown;
  published_at?: unknown;
  body?: unknown;
  draft?: unknown;
  prerelease?: unknown;
  assets?: unknown;
}

const releasesApi =
  'https://api.github.com/repos/Dubcodes/kairix-pimp/releases?per_page=20';
const releasePagePrefix =
  'https://github.com/Dubcodes/kairix-pimp/releases/tag/';
const assetPrefix =
  'https://github.com/Dubcodes/kairix-pimp/releases/download/';

function versionParts(value: string): [number, number, number, number] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-(\d+))?$/.exec(value.trim());
  if (!match) return null;
  return [
    Number(match[1]),
    Number(match[2]),
    Number(match[3]),
    Number(match[4] ?? 0),
  ];
}

export function comparePublicVersions(left: string, right: string): number {
  const leftParts = versionParts(left);
  const rightParts = versionParts(right);
  if (!leftParts || !rightParts) throw new Error('Invalid release version.');
  for (let index = 0; index < leftParts.length; index += 1) {
    if (leftParts[index] !== rightParts[index])
      return leftParts[index] - rightParts[index];
  }
  return 0;
}

function safeAsset(asset: GitHubAsset, tag: string) {
  if (
    typeof asset.name !== 'string' ||
    typeof asset.size !== 'number' ||
    asset.size <= 0 ||
    typeof asset.browser_download_url !== 'string' ||
    !asset.browser_download_url.startsWith(`${assetPrefix}${tag}/`) ||
    asset.browser_download_url.includes('?') ||
    asset.browser_download_url.includes('#')
  )
    return undefined;
  return {
    name: asset.name,
    size: asset.size,
    url: asset.browser_download_url,
  };
}

export async function checkPublicReleases(
  installedVersion: string,
  channel: 'Development' | 'Stable',
  request: typeof fetch = fetch,
): Promise<PublicRelease | null> {
  const response = await request(releasesApi, {
    headers: {
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok)
    throw new Error(`Public Releases API returned ${response.status}.`);
  const payload: unknown = await response.json();
  if (!Array.isArray(payload))
    throw new Error('Public Releases API returned an invalid response.');
  const releases = payload.flatMap((raw): PublicRelease[] => {
    const release = raw as GitHubRelease;
    if (
      release.draft !== false ||
      typeof release.prerelease !== 'boolean' ||
      typeof release.tag_name !== 'string' ||
      !release.tag_name.startsWith('v') ||
      !versionParts(release.tag_name) ||
      typeof release.published_at !== 'string' ||
      !Number.isFinite(Date.parse(release.published_at)) ||
      typeof release.html_url !== 'string' ||
      release.html_url !== `${releasePagePrefix}${release.tag_name}` ||
      !Array.isArray(release.assets)
    )
      return [];
    const classification = release.prerelease ? 'Development' : 'Stable';
    if (channel === 'Stable' && classification !== 'Stable') return [];
    const assets = release.assets.flatMap((asset) => {
      const validated = safeAsset(
        asset as GitHubAsset,
        release.tag_name as string,
      );
      return validated ? [validated] : [];
    });
    const version = release.tag_name.slice(1);
    const installer = assets.find(
      (asset) =>
        asset.name ===
        `Kairix-PIMP-Light-${version}-Windows-x64-Setup-Unsigned.exe`,
    );
    const checksum = assets.find((asset) => asset.name === 'SHA256SUMS.txt');
    const buildInfo = assets.find((asset) => asset.name === 'BUILD-INFO.txt');
    if (!installer || !checksum || !buildInfo) return [];
    return [
      {
        version,
        classification,
        publishedAt: release.published_at,
        notes: typeof release.body === 'string' ? release.body : '',
        releaseUrl: release.html_url,
        installerUrl: installer.url,
        installerSizeBytes: installer.size,
        checksumUrl: checksum.url,
        buildInfoUrl: buildInfo.url,
        newerThanInstalled:
          comparePublicVersions(version, installedVersion) > 0,
      },
    ];
  });
  releases.sort((left, right) =>
    comparePublicVersions(right.version, left.version),
  );
  return releases[0] ?? null;
}
