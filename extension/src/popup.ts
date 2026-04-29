// rongnote popup. Three states:
//   1. no server configured  → settings button
//   2. server set, vault locked → login form
//   3. unlocked → list of secrets matching current tab URL,
//                 'see all' fallback when nothing matches
//
// Each row has copy buttons (user / pass / TOTP). Clipboard auto-clears
// after 30s, same posture as the SPA's SecretEditor.

import { Api, ApiError, type Item, type ItemSummary } from './lib/api';
import {
  deriveAuthHash,
  deriveKekFromPassphrase,
  ensureReady,
  fromBase64,
  open,
  toBase64
} from './lib/crypto';
import { decryptSecret, urlMatches, type SecretPayload } from './lib/items';
import { generateCode, parseTotpInput } from './lib/totp';
import {
  clearVault,
  loadSettings,
  loadVault,
  saveSettings,
  saveVault,
  type Vault
} from './lib/store';

const $main = document.getElementById('main') as HTMLElement;
const $host = document.getElementById('host') as HTMLElement;
const $settings = document.getElementById('settings') as HTMLAnchorElement;
const $lock = document.getElementById('lock') as HTMLAnchorElement;
const $toast = document.getElementById('toast') as HTMLElement;

$settings.addEventListener('click', () => browser.runtime.openOptionsPage());
$lock.addEventListener('click', async () => {
  await clearVault();
  await render();
});

let toastTimer: number | undefined;
function toast(msg: string) {
  $toast.textContent = msg;
  $toast.classList.add('show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => $toast.classList.remove('show'), 1400);
}

function pingActivity() {
  browser.runtime.sendMessage({ type: 'activity' }).catch(() => {
    /* background may not be alive yet; the next ping will arm it */
  });
}

async function copyWithClear(value: string, label: string, ttlSeconds = 30) {
  await navigator.clipboard.writeText(value);
  pingActivity();
  toast(`copied ${label} · clears in ${ttlSeconds}s`);
  // Best-effort clipboard clear. If the user has copied something else by
  // then, we don't overwrite — checking is racy in MV3 popups but we try.
  setTimeout(async () => {
    try {
      const current = await navigator.clipboard.readText();
      if (current === value) await navigator.clipboard.writeText('');
    } catch {
      /* permissions revoked or popup closed; that's fine */
    }
  }, ttlSeconds * 1000);
}

async function getCurrentTabHost(): Promise<string> {
  const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
  if (!tab?.url) return '';
  try {
    return new URL(tab.url).hostname;
  } catch {
    return '';
  }
}

async function render() {
  await ensureReady();
  const settings = await loadSettings();
  const vault = await loadVault();
  const host = await getCurrentTabHost();
  $host.textContent = host || '';
  $lock.hidden = vault === null;

  if (!settings.server) {
    return renderNoServer();
  }
  if (!vault) {
    return renderLogin(settings.server, settings.email);
  }
  return renderUnlocked(settings.server, vault, host);
}

function renderNoServer() {
  $main.innerHTML = `
    <p class="muted">No server configured. Set the URL of your rongnote
    instance — for example <code>https://notes.example.com</code> — then
    sign in.</p>
    <div class="row" style="margin-top: 8px;">
      <button id="open-settings" class="primary">open settings</button>
    </div>
  `;
  document
    .getElementById('open-settings')!
    .addEventListener('click', () => browser.runtime.openOptionsPage());
}

function renderLogin(server: string, lastEmail: string) {
  $main.innerHTML = `
    <div class="muted" style="font-size: 11px;">${escapeHtml(server)}</div>
    <label>email</label>
    <input id="email" type="email" autocomplete="email" value="${escapeHtml(lastEmail)}">
    <label>passphrase</label>
    <input id="pass" type="password" autocomplete="current-password">
    <div class="err" id="err"></div>
    <div class="stage" id="stage"></div>
    <div class="row" style="margin-top: 8px; justify-content: flex-end; gap: 8px;">
      <button id="login" class="primary">sign in</button>
    </div>
  `;
  const $email = document.getElementById('email') as HTMLInputElement;
  const $pass = document.getElementById('pass') as HTMLInputElement;
  const $err = document.getElementById('err') as HTMLElement;
  const $stage = document.getElementById('stage') as HTMLElement;
  const $login = document.getElementById('login') as HTMLButtonElement;

  (lastEmail ? $pass : $email).focus();

  async function tryLogin() {
    $err.textContent = '';
    $login.disabled = true;
    try {
      const email = $email.value.trim();
      const passphrase = $pass.value;
      if (!email || !passphrase) {
        $err.textContent = 'email + passphrase required';
        return;
      }
      const api = new Api(server);

      $stage.textContent = 'precheck…';
      const pre = await api.precheck(email);

      $stage.textContent = 'deriving key (Argon2id ~1s)…';
      const salt = fromBase64(pre.passphrase_salt);
      const kek = await deriveKekFromPassphrase(passphrase, salt);
      const wrap = fromBase64(pre.master_wrap_passphrase);
      let masterKey: Uint8Array;
      try {
        masterKey = open(wrap, kek);
      } catch {
        $err.textContent = 'wrong passphrase';
        return;
      }

      $stage.textContent = 'sign in…';
      const authHash = deriveAuthHash(masterKey);
      const user = await api.login(email, toBase64(authHash));

      // Unwrap the X25519 private key with the master_key — needed to
      // decrypt sealed wraps from team-space items later.
      const privBlob = fromBase64(user.encrypted_private_key);
      const privateKey = open(privBlob, masterKey);

      const v: Vault = {
        master_key_b64: toBase64(masterKey),
        public_key_b64: user.public_key,
        private_key_b64: toBase64(privateKey),
        unlocked_at: Date.now()
      };
      await saveVault(v);
      await saveSettings({ server, email });
      await render();
    } catch (err) {
      $err.textContent =
        err instanceof ApiError
          ? err.message
          : err instanceof Error
            ? err.message
            : 'sign in failed';
    } finally {
      $login.disabled = false;
      $stage.textContent = '';
    }
  }

  $login.addEventListener('click', tryLogin);
  $pass.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') tryLogin();
  });
}

async function renderUnlocked(server: string, vault: Vault, host: string) {
  $main.innerHTML = `<div class="muted">loading secrets…</div>`;
  const api = new Api(server);
  let summaries: ItemSummary[] = [];
  try {
    summaries = await api.listItems({ type: 'secret' });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) {
      // Cookie expired or otherwise rejected. Bounce to login.
      await clearVault();
      await render();
      return;
    }
    $main.innerHTML = `<div class="danger">${escapeHtml(
      err instanceof Error ? err.message : 'list failed'
    )}</div>`;
    return;
  }

  // Fetch + decrypt each secret. Sequential to be gentle on the server;
  // n is small (<200 typical).
  const decoded: { summary: ItemSummary; payload: SecretPayload }[] = [];
  for (const s of summaries) {
    try {
      const full = await api.getItem(s.id);
      const payload = decryptSecret(full as Item, vault);
      if (payload) decoded.push({ summary: s, payload });
    } catch (err) {
      console.warn('skip', s.id, err);
    }
  }

  const matching = host
    ? decoded.filter((d) => urlMatches(d.payload.url, host))
    : [];

  let showAll = matching.length === 0;
  function paint() {
    const visible = showAll ? decoded : matching;
    if (visible.length === 0) {
      $main.innerHTML = `<div class="muted">no secrets in this vault.</div>`;
      return;
    }
    const list = visible
      .map(
        (d, i) => `
      <li>
        <div class="title">${escapeHtml(d.summary.title)}</div>
        ${d.payload.username ? `<div class="user">${escapeHtml(d.payload.username)}</div>` : ''}
        <div class="row copy-row">
          ${d.payload.username ? `<button data-i="${i}" data-act="user">user</button>` : ''}
          ${d.payload.password ? `<button data-i="${i}" data-act="pass">pass</button>` : ''}
          ${d.payload.totp_seed ? `<button data-i="${i}" data-act="totp">totp</button>` : ''}
        </div>
      </li>`
      )
      .join('');
    const banner =
      matching.length === 0 && host
        ? `<div class="muted" style="margin-bottom: 6px;">no match for <strong>${escapeHtml(host)}</strong> — showing all</div>`
        : matching.length > 0
          ? `<div class="row spaced" style="margin-bottom: 6px;">
               <span class="muted">${matching.length} match${matching.length === 1 ? '' : 'es'} for ${escapeHtml(host)}</span>
               <a href="#" id="show-all" class="muted">all (${decoded.length})</a>
             </div>`
          : '';
    $main.innerHTML = `${banner}<ul class="secrets">${list}</ul>`;

    document.getElementById('show-all')?.addEventListener('click', (e) => {
      e.preventDefault();
      showAll = true;
      paint();
    });

    $main.querySelectorAll<HTMLButtonElement>('button[data-act]').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const i = parseInt(btn.dataset.i!, 10);
        const item = visible[i];
        const act = btn.dataset.act!;
        try {
          if (act === 'user') await copyWithClear(item.payload.username, 'username');
          if (act === 'pass') await copyWithClear(item.payload.password, 'password');
          if (act === 'totp') {
            const cfg = parseTotpInput(item.payload.totp_seed);
            const { code } = await generateCode(cfg);
            await copyWithClear(code, 'TOTP code', 30);
          }
        } catch (err) {
          toast(err instanceof Error ? err.message : 'copy failed');
        }
      });
    });
  }
  paint();
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : c === '"' ? '&quot;' : '&#39;'
  );
}

render()
  .then(() => pingActivity())
  .catch((err) => {
    $main.innerHTML = `<div class="danger">${escapeHtml(
      err instanceof Error ? err.message : String(err)
    )}</div>`;
  });
