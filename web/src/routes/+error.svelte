<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';

  /// Custom error page so we can see the *actual* underlying error instead
  /// of SvelteKit's default "Internal Error" placeholder. Especially useful
  /// for diagnosing iOS-Safari-only client-side crashes that bubble up to
  /// the navigator.
  ///
  /// Anything stashed on window.__rongnote_lastError by global handlers is
  /// surfaced here too — SvelteKit eats unhandled promise rejections from
  /// onMount and turns them into a generic 500, so we have to capture the
  /// underlying error before it gets normalised.

  let lastError = $derived.by(() => {
    if (typeof window === 'undefined') return null;
    const w = window as unknown as {
      __rongnote_lastError?: { msg: string; stack?: string; src?: string };
    };
    return w.__rongnote_lastError ?? null;
  });

  function copyDetails() {
    const lines = [
      `status: ${page.status}`,
      `message: ${page.error?.message ?? '(no message)'}`,
      `path: ${page.url.pathname}`,
      `ua: ${navigator.userAgent}`,
      `time: ${new Date().toISOString()}`
    ];
    if (lastError) {
      lines.push('');
      lines.push(`captured: ${lastError.msg}`);
      if (lastError.src) lines.push(`source: ${lastError.src}`);
      if (lastError.stack) lines.push(`stack:\n${lastError.stack}`);
    }
    const text = lines.join('\n');
    void navigator.clipboard.writeText(text).then(
      () => alert('details copied'),
      () => prompt('copy this:', text)
    );
  }
</script>

<div class="page">
  <h1>{page.status}</h1>
  <p class="msg">{page.error?.message ?? 'Internal Error'}</p>

  <div class="meta">
    <div><span class="k">path</span> <code>{page.url.pathname}</code></div>
    {#if lastError}
      <div><span class="k">captured</span> <code>{lastError.msg}</code></div>
      {#if lastError.src}
        <div><span class="k">source</span> <code>{lastError.src}</code></div>
      {/if}
      {#if lastError.stack}
        <details>
          <summary>stack</summary>
          <pre>{lastError.stack}</pre>
        </details>
      {/if}
    {/if}
  </div>

  <div class="actions">
    <button type="button" onclick={() => location.reload()}>reload</button>
    <button type="button" onclick={() => goto('/login', { replaceState: true })}>login</button>
    <button type="button" onclick={copyDetails}>copy details</button>
  </div>
</div>

<style>
  .page {
    max-width: 600px;
    margin: 8vh auto;
    padding: 0 16px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 13px;
    color: var(--fg);
  }
  h1 { font-size: 48px; margin: 0 0 4px; line-height: 1; }
  .msg { font-size: 16px; color: var(--accent); margin: 0 0 24px; }
  .meta { display: flex; flex-direction: column; gap: 6px; margin-bottom: 24px; }
  .k {
    display: inline-block;
    width: 70px;
    color: var(--muted);
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.05em;
  }
  code {
    background: rgba(127, 127, 127, 0.12);
    padding: 1px 6px;
  }
  details { margin-top: 8px; }
  summary { cursor: pointer; color: var(--muted); }
  pre {
    background: rgba(127, 127, 127, 0.1);
    padding: 10px;
    overflow: auto;
    font-size: 11px;
    margin: 4px 0 0;
    max-height: 240px;
  }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .actions button {
    background: transparent;
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 6px 14px;
    font: inherit;
    cursor: pointer;
  }
  .actions button:hover { border-color: var(--fg); }
</style>
