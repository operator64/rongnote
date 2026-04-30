// CSV → ImportedSecret[]. Tolerant by design — header detection matches
// the column names emitted by Firefox, Chrome, Bitwarden, 1Password, and
// KeePass exports. The actual upload (encrypt + POST) lives in the
// route at /items/import.

export interface ImportedSecret {
  title: string;
  username: string;
  password: string;
  url: string;
  totp_seed: string;
  notes: string;
}

/// Strip UTF-8 BOM if present, then guess the delimiter from the first
/// non-empty line. Excel-on-Windows writes BOMs by default; locale-bound
/// exports occasionally use ; instead of ,.
function detectDelimiter(text: string): string {
  const firstLine = text.split(/\r?\n/).find((l) => l.length > 0) ?? '';
  const counts: Record<string, number> = {
    ',': (firstLine.match(/,/g) ?? []).length,
    ';': (firstLine.match(/;/g) ?? []).length,
    '\t': (firstLine.match(/\t/g) ?? []).length
  };
  let best = ',';
  let max = -1;
  for (const [d, n] of Object.entries(counts)) {
    if (n > max) {
      max = n;
      best = d;
    }
  }
  return best;
}

/// Minimal RFC 4180-ish parser. Handles quoted fields with commas,
/// embedded newlines, and "" → ".
export function parseCsv(input: string, delimiter?: string): string[][] {
  let text = input;
  if (text.charCodeAt(0) === 0xfeff) text = text.slice(1); // BOM
  const sep = delimiter ?? detectDelimiter(text);

  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let inQuotes = false;
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i += 2;
        } else {
          inQuotes = false;
          i++;
        }
      } else {
        field += c;
        i++;
      }
    } else if (c === '"' && field === '') {
      inQuotes = true;
      i++;
    } else if (c === sep) {
      row.push(field);
      field = '';
      i++;
    } else if (c === '\r') {
      i++;
    } else if (c === '\n') {
      row.push(field);
      field = '';
      rows.push(row);
      row = [];
      i++;
    } else {
      field += c;
      i++;
    }
  }
  if (field !== '' || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  while (rows.length > 0 && rows[rows.length - 1].every((f) => f === '')) {
    rows.pop();
  }
  return rows;
}

/// Header → secret-row mapping. Returns the mapped secrets plus a `format`
/// label for the UI ("firefox" / "chrome" / "bitwarden" / "1password" /
/// "generic"), used purely for the preview banner.
export interface ImportResult {
  secrets: ImportedSecret[];
  format: string;
  warnings: string[];
}

export function rowsToSecrets(rows: string[][]): ImportResult {
  const warnings: string[] = [];
  if (rows.length < 2) {
    return { secrets: [], format: 'unknown', warnings: ['file has no data rows'] };
  }
  const headers = rows[0].map((h) => h.trim().toLowerCase());

  const find = (...candidates: string[]): number => {
    for (const c of candidates) {
      const i = headers.indexOf(c);
      if (i >= 0) return i;
    }
    return -1;
  };

  const iUrl = find('url', 'login_uri', 'login_url', 'website', 'site', 'web site');
  const iUser = find(
    'username',
    'login',
    'user',
    'login_username',
    'email',
    'e-mail',
    'login email'
  );
  const iPass = find('password', 'pass', 'login_password');
  const iTitle = find('name', 'title', 'item', 'item name');
  const iNotes = find('notes', 'note', 'extra', 'comment', 'login_notes');
  const iTotp = find('totp', 'otpauth', 'authenticator', 'login_totp', 'one-time password');

  if (iUser < 0 || iPass < 0) {
    return {
      secrets: [],
      format: 'unknown',
      warnings: [
        `couldn't find "username" + "password" columns; got headers: ${headers.join(', ')}`
      ]
    };
  }

  // Heuristic format-naming for the preview banner.
  let format = 'generic';
  if (headers.includes('formactionorigin') || headers.includes('httprealm')) format = 'firefox';
  else if (headers.includes('login_uri') || headers.includes('reprompt')) format = 'bitwarden';
  else if (headers.length === 5 && iTitle >= 0 && iUrl >= 0 && iUser >= 0 && iPass >= 0)
    format = 'chrome';
  else if (headers.includes('1password unlock secret') || headers.includes('vault')) format = '1password';

  const out: ImportedSecret[] = [];
  for (let r = 1; r < rows.length; r++) {
    const row = rows[r];
    const url = iUrl >= 0 ? (row[iUrl] ?? '').trim() : '';
    const username = (row[iUser] ?? '').trim();
    const password = row[iPass] ?? '';
    if (!username && !password) continue;
    const explicitTitle = iTitle >= 0 ? (row[iTitle] ?? '').trim() : '';
    const title = explicitTitle || titleFromUrl(url) || 'imported';
    out.push({
      title,
      username,
      password,
      url,
      notes: iNotes >= 0 ? (row[iNotes] ?? '') : '',
      totp_seed: iTotp >= 0 ? (row[iTotp] ?? '').trim() : ''
    });
  }
  return { secrets: out, format, warnings };
}

function titleFromUrl(url: string): string {
  if (!url) return '';
  let s = url.trim();
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(s)) s = 'https://' + s;
  try {
    return new URL(s).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

export function todayYMD(): string {
  const d = new Date();
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
