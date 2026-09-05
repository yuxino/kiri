/** Build-time data only. Pinned to one commit; no downloaded JavaScript is executed. */
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('.', import.meta.url));
const names = ['header-0.b64', 'manifest.json', 'sidebar-0.b64', 'sidebar-1.b64', 'sidebar-2.b64', 'sidebar-3.b64'];
const digest = data => createHash('sha256').update(data).digest('hex');

export async function ensureArtwork({ directory = root, offline = false, fetcher = globalThis.fetch } = {}) {
  const lock = JSON.parse(readFileSync(join(directory, 'artwork.lock.json'), 'utf8'));
  if (!/^[a-f0-9]{40}$/.test(lock.commit ?? '') ||
      JSON.stringify(Object.keys(lock.files ?? {}).sort()) !== JSON.stringify(names)) {
    throw new Error('Invalid installer artwork lock');
  }
  const folder = join(directory, 'artwork');
  for (const name of names) {
    const entry = lock.files[name];
    if (!/^[a-f0-9]{64}$/.test(entry.sha256 ?? '') || !Number.isInteger(entry.size) || entry.size < 1 || entry.size > 16000) {
      throw new Error(`Invalid artwork lock entry: ${name}`);
    }
    const destination = join(folder, name);
    if (existsSync(destination)) {
      const data = readFileSync(destination);
      if (data.length !== entry.size || digest(data) !== entry.sha256) throw new Error(`Artwork checksum mismatch: ${name}`);
      continue;
    }
    if (offline) throw new Error(`Missing artwork: ${name}; run build.mjs once with network access`);
    const url = `https://raw.githubusercontent.com/yuxino/kiri/${lock.commit}/src-tauri/installer-theme/artwork/${name}`;
    const response = await fetcher(url, { redirect: 'error', signal: AbortSignal.timeout(15000) });
    if (!response.ok || !response.body) throw new Error(`Artwork download failed: ${name} (${response.status})`);
    const chunks = []; let size = 0;
    for await (const chunk of response.body) {
      size += chunk.length;
      if (size > entry.size) throw new Error(`Oversized artwork download: ${name}`);
      chunks.push(Buffer.from(chunk));
    }
    const data = Buffer.concat(chunks);
    if (size !== entry.size || digest(data) !== entry.sha256) throw new Error(`Artwork checksum mismatch: ${name}`);
    mkdirSync(folder, { recursive: true });
    const temporary = `${destination}.${process.pid}.tmp`;
    try { writeFileSync(temporary, data); renameSync(temporary, destination); }
    finally { rmSync(temporary, { force: true }); }
  }
}
