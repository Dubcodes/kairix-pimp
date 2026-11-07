import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$/;

export function parseVersion(value) {
  const match = SEMVER.exec(value);
  if (!match) throw new Error(`Invalid SemVer: ${value}`);
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] ?? null,
  };
}

export function compareVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  for (const key of ['major', 'minor', 'patch']) {
    if (a[key] !== b[key]) return a[key] < b[key] ? -1 : 1;
  }
  if (a.prerelease === b.prerelease) return 0;
  if (a.prerelease === null) return 1;
  if (b.prerelease === null) return -1;
  return a.prerelease.localeCompare(b.prerelease, 'en', { numeric: true });
}

export function displayVersion(value) {
  const version = parseVersion(value);
  return version.patch === 0 && version.prerelease === null
    ? `${version.major}.${version.minor}`
    : value;
}

export function validateClassification(version, classification) {
  const parsed = parseVersion(version);
  if (!['development', 'stable'].includes(classification)) {
    throw new Error('Classification must be development or stable.');
  }
  if (parsed.prerelease !== null) {
    throw new Error('New releases must use plain three-component SemVer.');
  }
  if (
    classification === 'stable' &&
    (parsed.major === 0 || parsed.minor !== 0)
  ) {
    throw new Error(
      'Stable milestones must use a whole major version such as 1.0.0.',
    );
  }
  if (
    classification === 'development' &&
    parsed.major > 0 &&
    parsed.minor === 0
  ) {
    throw new Error(
      'A whole major .0 version requires an explicit stable promotion.',
    );
  }
  return true;
}

export function validateRepositoryVersions(root, expected, classification) {
  const packageVersion = JSON.parse(
    fs.readFileSync(path.join(root, 'package.json'), 'utf8'),
  ).version;
  const desktopVersion = JSON.parse(
    fs.readFileSync(path.join(root, 'apps/desktop/package.json'), 'utf8'),
  ).version;
  const tauriVersion = JSON.parse(
    fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'),
  ).version;
  const cargo = fs.readFileSync(
    path.join(root, 'src-tauri/Cargo.toml'),
    'utf8',
  );
  const cargoVersion = /^version = "([^"]+)"/m.exec(cargo)?.[1];
  const versions = [packageVersion, desktopVersion, tauriVersion, cargoVersion];
  if (versions.some((version) => version !== expected)) {
    throw new Error(`Version metadata disagrees: ${versions.join(', ')}`);
  }
  validateClassification(expected, classification);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, expected, classification] = process.argv.slice(2);
  if (command === 'compare') {
    const comparison = compareVersions(expected, classification);
    process.stdout.write(
      comparison < 0 ? 'older\n' : comparison > 0 ? 'newer\n' : 'equal\n',
    );
    process.exit(0);
  }
  if (command !== 'validate') {
    throw new Error(
      'Usage: node tools/release-policy.mjs validate <version> <classification> or compare <candidate> <current>',
    );
  }
  validateRepositoryVersions(process.cwd(), expected, classification);
  process.stdout.write(`${displayVersion(expected)} ${classification}\n`);
}
