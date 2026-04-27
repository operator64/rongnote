// RFC 6238 TOTP via Web Crypto API. SHA-1, 6 digits, 30s window — matches
// every authenticator app you've ever used.
//
// Accepts either:
//   - a plain base32 secret ("JBSWY3DPEHPK3PXP")
//   - or a full otpauth:// URI (e.g. exported from another app)

const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export class TotpError extends Error {}

export interface TotpConfig {
  secret: Uint8Array;
  digits: number;
  period: number;
  algorithm: 'SHA-1' | 'SHA-256' | 'SHA-512';
  label?: string;
  issuer?: string;
}

export function parseTotpInput(input: string): TotpConfig {
  const trimmed = input.trim();
  if (!trimmed) throw new TotpError('empty');

  if (trimmed.startsWith('otpauth://')) {
    return parseOtpAuth(trimmed);
  }

  // Plain base32 — strip whitespace + dashes, uppercase.
  const cleaned = trimmed.replace(/[\s-]+/g, '').toUpperCase();
  return {
    secret: base32Decode(cleaned),
    digits: 6,
    period: 30,
    algorithm: 'SHA-1'
  };
}

function parseOtpAuth(uri: string): TotpConfig {
  const url = new URL(uri);
  if (url.protocol !== 'otpauth:') throw new TotpError('not an otpauth uri');
  if (url.host !== 'totp') throw new TotpError('hotp not supported');
  const labelPath = decodeURIComponent(url.pathname.replace(/^\//, ''));
  const params = url.searchParams;
  const secret = params.get('secret');
  if (!secret) throw new TotpError('missing secret');
  return {
    secret: base32Decode(secret.replace(/[\s-]+/g, '').toUpperCase()),
    digits: parseInt(params.get('digits') ?? '6', 10),
    period: parseInt(params.get('period') ?? '30', 10),
    algorithm: ((params.get('algorithm') ?? 'SHA1').toUpperCase().replace(
      /^SHA(\d+)$/,
      'SHA-$1'
    ) as TotpConfig['algorithm']),
    label: labelPath,
    issuer: params.get('issuer') ?? undefined
  };
}

export async function generateCode(
  config: TotpConfig,
  now: number = Date.now()
): Promise<{ code: string; secondsRemaining: number }> {
  const counter = Math.floor(now / 1000 / config.period);
  const counterBuf = new ArrayBuffer(8);
  const view = new DataView(counterBuf);
  // i64 big-endian. JS bitwise is i32, so split high/low.
  view.setUint32(0, Math.floor(counter / 0x100000000), false);
  view.setUint32(4, counter >>> 0, false);

  const key = await crypto.subtle.importKey(
    'raw',
    config.secret as BufferSource,
    { name: 'HMAC', hash: config.algorithm },
    false,
    ['sign']
  );
  const sigBuf = await crypto.subtle.sign('HMAC', key, counterBuf);
  const sig = new Uint8Array(sigBuf);
  const offset = sig[sig.length - 1] & 0xf;
  const truncated =
    ((sig[offset] & 0x7f) << 24) |
    (sig[offset + 1] << 16) |
    (sig[offset + 2] << 8) |
    sig[offset + 3];
  const mod = 10 ** config.digits;
  const code = (truncated % mod).toString().padStart(config.digits, '0');

  const secondsElapsed = Math.floor(now / 1000) % config.period;
  const secondsRemaining = config.period - secondsElapsed;
  return { code, secondsRemaining };
}

function base32Decode(s: string): Uint8Array {
  // Strip RFC4648 padding if present.
  s = s.replace(/=+$/, '');
  const out: number[] = [];
  let bits = 0;
  let value = 0;
  for (const c of s) {
    const idx = BASE32_ALPHABET.indexOf(c);
    if (idx < 0) throw new TotpError(`invalid base32 character: ${c}`);
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}
