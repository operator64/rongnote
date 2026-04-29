// RFC 6238 TOTP via Web Crypto. Mirrors web/src/lib/totp.ts but trimmed
// to the minimum the popup needs: parse a base32 seed (or otpauth:// URI),
// compute the current 6-digit code.

const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

function base32Decode(s: string): Uint8Array {
  const cleaned = s.replace(/[\s=-]+/g, '').toUpperCase();
  const out: number[] = [];
  let bits = 0;
  let acc = 0;
  for (const ch of cleaned) {
    const v = ALPHABET.indexOf(ch);
    if (v < 0) throw new Error('invalid base32');
    acc = (acc << 5) | v;
    bits += 5;
    if (bits >= 8) {
      out.push((acc >> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}

export interface TotpConfig {
  secret: Uint8Array;
  digits: number;
  period: number;
  algorithm: 'SHA-1' | 'SHA-256' | 'SHA-512';
}

export function parseTotpInput(input: string): TotpConfig {
  const trimmed = input.trim();
  if (trimmed.startsWith('otpauth://')) {
    const u = new URL(trimmed);
    const secret = u.searchParams.get('secret');
    if (!secret) throw new Error('otpauth missing secret');
    const digits = parseInt(u.searchParams.get('digits') ?? '6', 10);
    const period = parseInt(u.searchParams.get('period') ?? '30', 10);
    const algo = (u.searchParams.get('algorithm') ?? 'SHA1').toUpperCase();
    return {
      secret: base32Decode(secret),
      digits,
      period,
      algorithm: algo === 'SHA256' ? 'SHA-256' : algo === 'SHA512' ? 'SHA-512' : 'SHA-1'
    };
  }
  return {
    secret: base32Decode(trimmed),
    digits: 6,
    period: 30,
    algorithm: 'SHA-1'
  };
}

export async function generateCode(cfg: TotpConfig): Promise<{ code: string; secondsRemaining: number }> {
  const now = Math.floor(Date.now() / 1000);
  const counter = Math.floor(now / cfg.period);
  const buf = new ArrayBuffer(8);
  const view = new DataView(buf);
  view.setUint32(0, Math.floor(counter / 0x1_0000_0000));
  view.setUint32(4, counter & 0xffff_ffff);
  const key = await crypto.subtle.importKey(
    'raw',
    cfg.secret as BufferSource,
    { name: 'HMAC', hash: cfg.algorithm },
    false,
    ['sign']
  );
  const mac = new Uint8Array(await crypto.subtle.sign('HMAC', key, buf));
  const offset = mac[mac.length - 1] & 0x0f;
  const bin =
    ((mac[offset] & 0x7f) << 24) |
    (mac[offset + 1] << 16) |
    (mac[offset + 2] << 8) |
    mac[offset + 3];
  const code = (bin % 10 ** cfg.digits).toString().padStart(cfg.digits, '0');
  const secondsRemaining = cfg.period - (now % cfg.period);
  return { code, secondsRemaining };
}
