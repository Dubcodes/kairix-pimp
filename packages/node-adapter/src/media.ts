import { execFile } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import type { TechnicalMetadata } from '@kairix-pimp/core';

const exec = promisify(execFile);
export async function probeMedia(filePath: string): Promise<TechnicalMetadata> {
  const { stdout } = await exec(
    'ffprobe',
    [
      '-v',
      'error',
      '-print_format',
      'json',
      '-show_format',
      '-show_streams',
      filePath,
    ],
    { maxBuffer: 4_000_000 },
  );
  const data = JSON.parse(stdout) as {
    format?: { duration?: string; size?: string };
    streams?: Array<{
      codec_type?: string;
      codec_name?: string;
      width?: number;
      height?: number;
      avg_frame_rate?: string;
    }>;
  };
  const video = data.streams?.find((stream) => stream.codec_type === 'video');
  const [numerator, denominator] = (video?.avg_frame_rate ?? '0/1')
    .split('/')
    .map(Number);
  return {
    durationSeconds: Number(data.format?.duration) || undefined,
    sizeBytes: Number(data.format?.size) || 0,
    width: video?.width,
    height: video?.height,
    codec: video?.codec_name,
    frameRate: denominator ? numerator / denominator : undefined,
  };
}

export async function generateThumbnail(
  filePath: string,
  outputPath: string,
  atSeconds = 1,
): Promise<void> {
  await mkdir(path.dirname(outputPath), { recursive: true });
  await exec('ffmpeg', [
    '-v',
    'error',
    '-y',
    '-ss',
    String(Math.max(0, atSeconds)),
    '-i',
    filePath,
    '-frames:v',
    '1',
    '-vf',
    'scale=640:-2',
    outputPath,
  ]);
}
