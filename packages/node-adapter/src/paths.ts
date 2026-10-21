import { realpath, stat } from 'node:fs/promises';
import path from 'node:path';

export function assertSafeRelativePath(relativePath: string): string {
  if (
    !relativePath ||
    path.isAbsolute(relativePath) ||
    relativePath.includes('\0')
  )
    throw new Error('Path must be a safe relative path');
  const normalized = path.normalize(relativePath);
  if (normalized === '..' || normalized.startsWith(`..${path.sep}`))
    throw new Error('Path escapes the configured root');
  return normalized;
}

export function resolveWithin(root: string, relativePath: string): string {
  const resolvedRoot = path.resolve(root);
  const resolved = path.resolve(
    resolvedRoot,
    assertSafeRelativePath(relativePath),
  );
  const relation = path.relative(resolvedRoot, resolved);
  if (
    relation === '..' ||
    relation.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relation)
  )
    throw new Error('Path escapes the configured root');
  return resolved;
}

export async function assertExistingPathWithin(
  root: string,
  candidate: string,
): Promise<string> {
  const [realRoot, realCandidate] = await Promise.all([
    realpath(root),
    realpath(candidate),
  ]);
  const relation = path.relative(realRoot, realCandidate);
  if (
    relation === '..' ||
    relation.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relation)
  )
    throw new Error('Path is outside the configured root');
  await stat(realCandidate);
  return realCandidate;
}
