import { loadSettings, saveSettings } from './lib/store';

const $server = document.getElementById('server') as HTMLInputElement;
const $email = document.getElementById('email') as HTMLInputElement;
const $save = document.getElementById('save') as HTMLButtonElement;
const $ok = document.getElementById('ok') as HTMLElement;
const $err = document.getElementById('err') as HTMLElement;

(async () => {
  const s = await loadSettings();
  $server.value = s.server;
  $email.value = s.email;
})();

$save.addEventListener('click', async () => {
  $ok.textContent = '';
  $err.textContent = '';
  let url = $server.value.trim();
  if (!url) {
    $err.textContent = 'server URL required';
    return;
  }
  // Add https:// if scheme missing.
  if (!/^https?:\/\//i.test(url)) url = 'https://' + url;
  try {
    const u = new URL(url);
    url = `${u.protocol}//${u.host}`;
  } catch {
    $err.textContent = 'not a valid URL';
    return;
  }
  await saveSettings({ server: url, email: $email.value.trim() });
  $ok.textContent = 'saved';
});
