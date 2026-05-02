<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import CalendarWidget from '$lib/dashboard/CalendarWidget.svelte';
  import ListWidget from '$lib/dashboard/ListWidget.svelte';
  import SettingsModal from '$lib/dashboard/SettingsModal.svelte';
  import TransitWidget from '$lib/dashboard/TransitWidget.svelte';
  import WeatherWidget from '$lib/dashboard/WeatherWidget.svelte';
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import { vault } from '$lib/vault.svelte';

  /// Always-live dashboard. Pauses the vault's idle-lock while the route
  /// is mounted and resumes it on destroy. The user can lock manually
  /// from the header. Widgets each own their own polling cadence — the
  /// dashboard itself just composes the layout.

  let settingsOpen = $state(false);
  let now = $state(new Date());

  onMount(() => {
    dashboardSettings.load();
    vault.pauseIdle();

    const tick = setInterval(() => (now = new Date()), 30_000);

    // Make sure spaces + items are hydrated so the calendar / list
    // widgets have data to read from.
    if (spaces.list.length === 0) void spaces.refresh();
    if (items.list.length === 0) void items.refresh();

    return () => {
      clearInterval(tick);
    };
  });

  onDestroy(() => {
    vault.resumeIdle();
  });

  function lockNow() {
    vault.lock();
    goto('/items', { replaceState: true });
  }

  async function refreshAll() {
    await items.refresh();
    // Widgets watch items.list / their own timers; the items.refresh
    // above is enough to repaint calendar + list. Weather + transit
    // refresh themselves on the same trigger via their internal hooks.
  }

  let nowLabel = $derived(
    now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
  );
  let dateLabel = $derived(
    now.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })
  );
</script>

<div class="page">
  <header class="bar">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <span class="title">dashboard</span>
    <span class="muted small">{dateLabel} · {nowLabel}</span>
    <span class="grow"></span>
    <span class="live-badge" title="auto-lock pausiert solang du auf dem dashboard bist">
      ● always-live
    </span>
    <button type="button" onclick={refreshAll} title="reload data">↻ refresh</button>
    <button type="button" onclick={() => (settingsOpen = true)} title="settings">⚙</button>
    <button type="button" onclick={lockNow} title="vault sperren">🔒 lock</button>
  </header>

  <div class="grid">
    <CalendarWidget />
    <ListWidget />
    <WeatherWidget />
    <TransitWidget />
  </div>
</div>

{#if settingsOpen}
  <SettingsModal onClose={() => (settingsOpen = false)} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .bar {
    height: 36px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
  }
  .bar .title { font-weight: 600; }
  .bar .small { font-size: 12px; }
  .bar .grow { flex: 1; }
  .bar button {
    background: transparent;
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 2px 8px;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .bar button:hover { border-color: var(--fg); }
  .live-badge {
    color: var(--accent);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
    gap: 0;
  }
  .grid > :global(section) {
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    border-top: none;
    border-left: none;
  }
  .grid > :global(section:nth-child(2n)) { border-right: none; }
  .grid > :global(section:nth-last-child(-n+2)) { border-bottom: none; }

  /* iPad portrait — single column */
  @media (max-width: 820px) {
    .grid {
      grid-template-columns: 1fr;
      grid-template-rows: repeat(4, minmax(220px, 1fr));
    }
    .grid > :global(section) {
      border-right: none !important;
      border-bottom: 1px solid var(--border);
    }
    .grid > :global(section:last-child) { border-bottom: none; }
  }
</style>
