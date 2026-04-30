<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, ApiError } from '$lib/api';
  import { generateItemKey, seal, toBase64, utf8Encode } from '$lib/crypto';
  import { wrapItemKey } from '$lib/itemCrypto';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import { vault } from '$lib/vault.svelte';
  import {
    parseCsv,
    rowsToSecrets,
    todayYMD,
    type ImportResult,
    type ImportedSecret
  } from '$lib/csvImport';

  type Stage = 'pick' | 'preview' | 'importing' | 'done';

  let stage = $state<Stage>('pick');
  let parsed = $state<ImportResult | null>(null);
  let filename = $state('');
  let error = $state('');

  let total = $state(0);
  let done = $state(0);
  let createdCount = $state(0);
  let skippedCount = $state(0);
  let failedCount = $state(0);
  let logLines = $state<string[]>([]);

  async function onPicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    filename = file.name;
    await loadFile(file);
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    filename = file.name;
    await loadFile(file);
  }
  function onDragOver(e: DragEvent) {
    e.preventDefault();
  }

  async function loadFile(file: File) {
    error = '';
    try {
      const text = await file.text();
      const rows = parseCsv(text);
      const result = rowsToSecrets(rows);
      if (result.secrets.length === 0) {
        error =
          result.warnings[0] ?? 'no secrets detected in this file';
        return;
      }
      parsed = result;
      stage = 'preview';
    } catch (err) {
      error = err instanceof Error ? err.message : 'parse failed';
    }
  }

  function reset() {
    stage = 'pick';
    parsed = null;
    filename = '';
    error = '';
    total = 0;
    done = 0;
    createdCount = 0;
    skippedCount = 0;
    failedCount = 0;
    logLines = [];
  }

  async function startImport() {
    if (!parsed) return;
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      error = 'vault locked';
      return;
    }
    const space = spaces.active;
    if (!space) {
      error = 'no active space';
      return;
    }
    stage = 'importing';
    total = parsed.secrets.length;
    done = 0;
    createdCount = 0;
    skippedCount = 0;
    failedCount = 0;
    logLines = [];

    // Existing-title set so we don't double-import on retries.
    let existingTitles = new Set<string>();
    try {
      const existing = await api.listItems({
        type: 'secret',
        space_id: space.id
      });
      existingTitles = new Set(existing.map((i) => i.title.toLowerCase()));
    } catch (err) {
      logLines = [
        `couldn't fetch existing secrets for dupe-check: ${err instanceof Error ? err.message : 'unknown'} — proceeding anyway`,
        ...logLines
      ];
    }

    const path = `/imported/${todayYMD()}`;

    for (const sec of parsed.secrets) {
      done++;
      const key = sec.title.toLowerCase();
      if (existingTitles.has(key)) {
        skippedCount++;
        continue;
      }
      try {
        await uploadOne(sec, path, space.id);
        existingTitles.add(key);
        createdCount++;
      } catch (err) {
        failedCount++;
        logLines = [
          `${sec.title}: ${err instanceof Error ? err.message : 'failed'}`,
          ...logLines
        ];
      }
    }
    stage = 'done';
  }

  async function uploadOne(sec: ImportedSecret, path: string, spaceId: string) {
    const itemKey = generateItemKey();
    const payload = JSON.stringify({
      username: sec.username,
      password: sec.password,
      url: sec.url,
      totp_seed: sec.totp_seed,
      notes: sec.notes
    });
    const encryptedBody = seal(utf8Encode(payload), itemKey);
    const wrap = await wrapItemKey(itemKey, spaceId, vault.masterKey!);
    const created = await api.createItem({
      type: 'secret',
      title: sec.title,
      encrypted_body: toBase64(encryptedBody),
      ...wrap,
      tags: ['imported'],
      path,
      space_id: spaceId
    });
    items.upsert(created);
  }

  let progressPct = $derived(total > 0 ? Math.round((done / total) * 100) : 0);
</script>

<div class="page">
  <div class="head row">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <span class="grow"></span>
    <span class="muted">import secrets</span>
  </div>

  {#if stage === 'pick'}
    <div class="body">
      <h1>import secrets from CSV</h1>
      <p class="muted">
        Each row becomes a new <strong>secret</strong> item in your active
        space, tagged <code>#imported</code> at <code>/imported/{todayYMD()}</code>.
        The file is parsed locally; nothing leaves the browser until each
        row is encrypted with a per-item key.
      </p>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="drop" ondrop={onDrop} ondragover={onDragOver}>
        <p>drop a CSV file here</p>
        <p class="muted">or</p>
        <label class="file-pick">
          <input type="file" accept=".csv,text/csv" onchange={onPicked} />
          <span>browse</span>
        </label>
      </div>

      {#if error}<div class="err">{error}</div>{/if}

      <details style="margin-top: 24px;">
        <summary class="muted">where do I get a CSV?</summary>
        <ul class="hint">
          <li><strong>Firefox:</strong> <code>about:logins</code> → ⋯ menu top-right →
            <em>"Logins exportieren…"</em>. Saves <code>logins.csv</code>.</li>
          <li><strong>Chrome / Edge:</strong> <code>chrome://password-manager/settings</code> →
            <em>Download passwords</em>. Saves <code>Chrome Passwords.csv</code>.</li>
          <li><strong>Bitwarden:</strong> <em>Tools → Export vault → File format CSV</em>.</li>
          <li><strong>1Password:</strong> Family/Teams plans only — <em>1Password.com → Export</em>.</li>
          <li><strong>KeePass:</strong> <em>File → Export → CSV (RFC 4180)</em>.</li>
        </ul>
      </details>
    </div>
  {:else if stage === 'preview' && parsed}
    <div class="body">
      <h1>preview</h1>
      <p class="muted">
        <strong>{parsed.secrets.length}</strong> secrets detected
        ({parsed.format} format) from <code>{filename}</code>.
        Existing items with the same title will be <em>skipped</em>.
      </p>
      <table class="preview">
        <thead>
          <tr><th>title</th><th>username</th><th>url</th></tr>
        </thead>
        <tbody>
          {#each parsed.secrets.slice(0, 50) as sec, i (i)}
            <tr>
              <td>{sec.title}</td>
              <td class="muted">{sec.username}</td>
              <td class="muted url">{sec.url}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if parsed.secrets.length > 50}
        <p class="muted">… and {parsed.secrets.length - 50} more</p>
      {/if}
      <div class="actions">
        <button type="button" onclick={reset}>cancel</button>
        <button type="button" class="primary" onclick={startImport}>
          import {parsed.secrets.length} secrets
        </button>
      </div>
    </div>
  {:else if stage === 'importing'}
    <div class="body">
      <h1>importing…</h1>
      <p class="muted">{done} / {total} processed</p>
      <div class="progress">
        <div class="progress-fill" style="width: {progressPct}%;"></div>
      </div>
      <p class="counts muted">
        {createdCount} created · {skippedCount} skipped (duplicate) · {failedCount} failed
      </p>
      {#if logLines.length > 0}
        <pre class="log">{logLines.join('\n')}</pre>
      {/if}
    </div>
  {:else if stage === 'done'}
    <div class="body">
      <h1>done</h1>
      <p>
        <strong>{createdCount}</strong> imported,
        <strong>{skippedCount}</strong> skipped (duplicate title),
        <strong>{failedCount}</strong> failed.
      </p>
      {#if logLines.length > 0}
        <details>
          <summary class="muted">{failedCount} failures</summary>
          <pre class="log">{logLines.join('\n')}</pre>
        </details>
      {/if}
      <div class="actions">
        <button type="button" onclick={reset}>import another</button>
        <button type="button" class="primary" onclick={() => goto('/items')}>
          back to items
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
    flex-shrink: 0;
  }
  .body {
    max-width: 720px;
    padding: 24px;
  }
  h1 {
    font-size: 18px;
    margin: 0 0 8px;
    font-weight: 600;
  }
  p {
    margin: 0 0 16px;
    line-height: 1.55;
  }
  .err {
    color: var(--danger);
    margin-top: 12px;
  }
  .drop {
    margin-top: 24px;
    border: 1px dashed var(--border);
    padding: 32px 16px;
    text-align: center;
    background: rgba(127, 127, 127, 0.04);
  }
  .drop p { margin: 4px 0; }
  .file-pick {
    display: inline-block;
    margin-top: 8px;
  }
  .file-pick input { display: none; }
  .file-pick span {
    display: inline-block;
    padding: 4px 12px;
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .file-pick span:hover { border-color: var(--fg); }
  .hint { margin: 0; padding-left: 18px; line-height: 1.7; }
  .hint code { background: rgba(127,127,127,0.10); padding: 0 4px; }

  table.preview {
    border-collapse: collapse;
    width: 100%;
    margin: 8px 0;
    font-size: 12px;
  }
  table.preview th {
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-weight: normal;
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.05em;
  }
  table.preview td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 220px;
  }
  table.preview td.url { font-size: 11px; }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 16px;
    justify-content: flex-end;
  }
  .actions .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .progress {
    height: 6px;
    background: rgba(127, 127, 127, 0.12);
    border: 1px solid var(--border);
    margin: 12px 0;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .counts { font-size: 12px; }
  .log {
    background: rgba(127,127,127,0.08);
    border: 1px solid var(--border);
    padding: 8px 12px;
    margin-top: 8px;
    max-height: 200px;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 12px;
  }
</style>
