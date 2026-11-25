import type { MediaItem } from './bridge';

export type LibraryMediaSort =
  | 'ingested-newest'
  | 'ingested-oldest'
  | 'recorded-newest'
  | 'recorded-oldest'
  | 'filename-asc'
  | 'filename-desc';

const textOrder = (left: string, right: string) =>
  left.localeCompare(right, undefined, {
    numeric: true,
    sensitivity: 'base',
  });

const stableIdentityOrder = (left: MediaItem, right: MediaItem) =>
  textOrder(left.originalFilename, right.originalFilename) ||
  textOrder(left.relativePath, right.relativePath) ||
  textOrder(left.id, right.id);

const dateOrder = (
  left: MediaItem,
  right: MediaItem,
  value: (item: MediaItem) => string | undefined,
  newestFirst: boolean,
) => {
  const leftValue = value(left);
  const rightValue = value(right);
  if (!leftValue && !rightValue) return stableIdentityOrder(left, right);
  if (!leftValue) return 1;
  if (!rightValue) return -1;
  const compared = textOrder(leftValue, rightValue);
  return (
    (newestFirst ? -compared : compared) || stableIdentityOrder(left, right)
  );
};

export const sortLibraryMedia = (
  media: readonly MediaItem[],
  sort: LibraryMediaSort,
) =>
  [...media].sort((left, right) => {
    switch (sort) {
      case 'ingested-oldest':
        return dateOrder(left, right, (item) => item.ingestDate, false);
      case 'recorded-newest':
        return dateOrder(left, right, (item) => item.captureDate, true);
      case 'recorded-oldest':
        return dateOrder(left, right, (item) => item.captureDate, false);
      case 'filename-asc':
        return stableIdentityOrder(left, right);
      case 'filename-desc':
        return (
          textOrder(right.originalFilename, left.originalFilename) ||
          textOrder(left.relativePath, right.relativePath) ||
          textOrder(left.id, right.id)
        );
      case 'ingested-newest':
      default:
        return dateOrder(left, right, (item) => item.ingestDate, true);
    }
  });
