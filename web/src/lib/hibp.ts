// HIBP "Pwned Passwords" range query via k-anonymity.
//
// We send only the first 5 hex chars of SHA-1(password) to the HIBP API,
// receive ~500 hash suffixes with their breach counts, and check locally
// whether ours is among them. The full hash never leaves the browser.
// CORS-allowed by HIBP, no server-side proxy needed.

const ENDPOINT = 'https://api.pwnedpasswords.com/range/';

export class HibpError extends Error {}

export async function pwnedCount(password: string): Promise<number> {
  if (!password) return 0;
  const buf = await crypto.subtle.digest('SHA-1', new TextEncoder().encode(password));
  const hex = Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0'))
    .join('')
    .toUpperCase();
  const prefix = hex.slice(0, 5);
  const suffix = hex.slice(5);
  let res: Response;
  try {
    res = await fetch(`${ENDPOINT}${prefix}`, {
      headers: { 'Add-Padding': 'true' }
    });
  } catch (err) {
    throw new HibpError(err instanceof Error ? err.message : 'network error');
  }
  if (!res.ok) {
    throw new HibpError(`HIBP API ${res.status}`);
  }
  const text = await res.text();
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    const sep = line.indexOf(':');
    if (sep < 0) continue;
    if (line.slice(0, sep) === suffix) {
      const count = parseInt(line.slice(sep + 1), 10);
      return Number.isFinite(count) ? count : 0;
    }
  }
  return 0;
}
