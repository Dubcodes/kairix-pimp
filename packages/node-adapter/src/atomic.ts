import { mkdir, open, rename, rm } from 'node:fs/promises';
import path from 'node:path';

export async function atomicWrite(
  filePath: string,
  data: string | Uint8Array,
): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  const temporary = `${filePath}.tmp-${process.pid}-${crypto.randomUUID()}`;
  const handle = await open(temporary, 'wx');
  try {
    await handle.writeFile(data);
    await handle.sync();
    await handle.close();
    await rename(temporary, filePath);
  } catch (error) {
    await handle.close().catch(() => undefined);
    await rm(temporary, { force: true });
    throw error;
  }
}
