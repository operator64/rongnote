// Cryptographically random password generator.

export interface GenOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
}

export const DEFAULT_OPTIONS: GenOptions = {
  length: 24,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true
};

const LOWER = 'abcdefghijklmnopqrstuvwxyz';
const UPPER = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
const DIGITS = '0123456789';
const SYMBOLS = "!@#$%^&*()-_=+[]{};:,.?";

export function generatePassword(opts: GenOptions): string {
  let charset = '';
  if (opts.lowercase) charset += LOWER;
  if (opts.uppercase) charset += UPPER;
  if (opts.digits) charset += DIGITS;
  if (opts.symbols) charset += SYMBOLS;
  if (!charset) throw new Error('at least one charset required');

  const len = Math.max(1, Math.min(128, opts.length));
  // Rejection-sample over 32-bit values so we don't bias the modulo.
  const max = Math.floor(0x100000000 / charset.length) * charset.length;
  const out: string[] = [];
  const buf = new Uint32Array(1);
  while (out.length < len) {
    crypto.getRandomValues(buf);
    if (buf[0] >= max) continue;
    out.push(charset[buf[0] % charset.length]);
  }
  return out.join('');
}

export interface PasswordStats {
  length: number;
  charsetSize: number;
  bitsOfEntropy: number;
}

export function entropyOf(password: string): PasswordStats {
  let charsetSize = 0;
  if (/[a-z]/.test(password)) charsetSize += 26;
  if (/[A-Z]/.test(password)) charsetSize += 26;
  if (/[0-9]/.test(password)) charsetSize += 10;
  if (/[^a-zA-Z0-9]/.test(password)) charsetSize += 30;
  const bits = password.length * Math.log2(Math.max(1, charsetSize));
  return { length: password.length, charsetSize, bitsOfEntropy: bits };
}
