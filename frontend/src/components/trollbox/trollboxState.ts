const KEY_OPEN = 'trollbox.open';
const KEY_LAST_SEEN = 'trollbox.lastSeenId';
const KEY_BLOCKED = 'trollbox.blocked';
const KEY_CHIME_MUTED = 'trollbox.chimeMuted';

const safeRead = (k: string): string | null => {
  try { return localStorage.getItem(k); } catch { return null; }
};
const safeWrite = (k: string, v: string) => {
  try { localStorage.setItem(k, v); } catch { /* ignore */ }
};

export const getOpen = (): boolean => safeRead(KEY_OPEN) === 'true';
export const setOpen = (v: boolean) => safeWrite(KEY_OPEN, String(v));

export const getLastSeenId = (): bigint => {
  const v = safeRead(KEY_LAST_SEEN);
  if (!v) return 0n;
  try { return BigInt(v); } catch { return 0n; }
};
export const setLastSeenId = (id: bigint) => safeWrite(KEY_LAST_SEEN, id.toString());

export const getBlocked = (): string[] => {
  const v = safeRead(KEY_BLOCKED);
  if (!v) return [];
  try { return JSON.parse(v) as string[]; } catch { return []; }
};
export const setBlocked = (list: string[]) =>
  safeWrite(KEY_BLOCKED, JSON.stringify(list));
export const addBlocked = (principalText: string) => {
  const cur = getBlocked();
  if (!cur.includes(principalText)) setBlocked([...cur, principalText]);
};
export const removeBlocked = (principalText: string) =>
  setBlocked(getBlocked().filter(p => p !== principalText));

export const getChimeMuted = (): boolean => safeRead(KEY_CHIME_MUTED) === 'true';
export const setChimeMuted = (v: boolean) => safeWrite(KEY_CHIME_MUTED, String(v));

export function subscribeBlocked(cb: (list: string[]) => void): () => void {
  const handler = (e: StorageEvent) => {
    if (e.key === KEY_BLOCKED) cb(getBlocked());
  };
  window.addEventListener('storage', handler);
  return () => window.removeEventListener('storage', handler);
}
