<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import CalendarWidget from '$lib/dashboard/CalendarWidget.svelte';
  import ClockWidget from '$lib/dashboard/ClockWidget.svelte';
  import ListWidget from '$lib/dashboard/ListWidget.svelte';
  import SettingsModal from '$lib/dashboard/SettingsModal.svelte';
  import TasksWidget from '$lib/dashboard/TasksWidget.svelte';
  import TransitWidget from '$lib/dashboard/TransitWidget.svelte';
  import WeatherWidget from '$lib/dashboard/WeatherWidget.svelte';
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import { vault } from '$lib/vault.svelte';

  /// Standalone dashboard route, sibling to /items so it renders without
  /// the items chrome (sidebar + list pane). Ideal for a wall-mounted
  /// kiosk display: pauses idle-lock while mounted, header offers manual
  /// refresh / settings / lock.

  let settingsOpen = $state(false);
  let now = $state(new Date());

  // Kiosk-style users get no "back to items" button — they have nothing
  // useful there. Personal users do.
  let kioskOnly = $derived(spaces.isKioskOnly);

  onMount(() => {
    dashboardSettings.load();
    vault.pauseIdle();

    const tick = setInterval(() => (now = new Date()), 30_000);

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
    goto('/login', { replaceState: true });
  }

  async function refreshAll() {
    await items.refresh();
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
    {#if !kioskOnly}
      <button type="button" onclick={() => goto('/items')}>← items</button>
    {/if}
    <span class="title">dashboard</span>
    <span class="muted small">{dateLabel} · {nowLabel}</span>
    <span class="grow"></span>
    <span class="live-badge" title="auto-lock pausiert solang du auf dem dashboard bist">
      ● always-live
    </span>
    <button type="button" onclick={refreshAll} title="reload data">↻ refresh</button>
    <button type="button" onclick={() => (settingsOpen = true)} title="settings">⚙</button>
    {#if !kioskOnly}
      <button type="button" onclick={lockNow} title="vault sperren">🔒 lock</button>
    {/if}
  </header>

  <div class="grid">
    <!-- top-left: calendar (full cell) -->
    <div class="cell">
      <CalendarWidget />
    </div>

    <!-- top-right: list | tasks (2 columns) -->
    <div class="cell split-cols">
      <ListWidget />
      <TasksWidget />
    </div>

    <!-- bottom-left: weather / clock (2 rows) -->
    <div class="cell split-rows">
      <WeatherWidget />
      <ClockWidget />
    </div>

    <!-- bottom-right: transit (full cell) -->
    <div class="cell">
      <TransitWidget />
    </div>
  </div>
</div>

{#if settingsOpen}
  <SettingsModal onClose={() => (settingsOpen = false)} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100vh;
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
  /* Each cell takes one quadrant of the grid. Outer borders give the
     tic-tac-toe lines between quadrants; the cell's own children draw
     the inner divider when split. */
  .cell {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  .cell:nth-child(2n) { border-right: none; }
  .cell:nth-last-child(-n+2) { border-bottom: none; }

  /* Single-widget cells let the section fill them. */
  .cell > :global(section) {
    flex: 1;
    min-height: 0;
    border: none;
  }

  /* Two-column split (e.g. list | tasks). Inner border between cols. */
  .cell.split-cols {
    flex-direction: row;
  }
  .cell.split-cols > :global(section) {
    flex: 1 1 0;
    min-width: 0;
    border-right: 1px solid var(--border);
  }
  .cell.split-cols > :global(section:last-child) {
    border-right: none;
  }

  /* Two-row split (e.g. weather / clock). Inner border between rows. */
  .cell.split-rows {
    flex-direction: column;
  }
  .cell.split-rows > :global(section) {
    flex: 1 1 0;
    min-height: 0;
    border-bottom: 1px solid var(--border);
  }
  .cell.split-rows > :global(section:last-child) {
    border-bottom: none;
  }

  /* iPad portrait — collapse everything into a single scrolling column.
     Splits also collapse so each widget gets its own row. */
  @media (max-width: 820px) {
    .grid {
      grid-template-columns: 1fr;
      grid-template-rows: repeat(4, minmax(220px, 1fr));
    }
    .cell {
      border-right: none !important;
      border-bottom: 1px solid var(--border);
    }
    .cell:last-child { border-bottom: none; }
    .cell.split-cols, .cell.split-rows {
      flex-direction: column;
    }
    .cell.split-cols > :global(section),
    .cell.split-rows > :global(section) {
      border-right: none;
      border-bottom: 1px solid var(--border);
      min-height: 180px;
    }
    .cell.split-cols > :global(section:last-child),
    .cell.split-rows > :global(section:last-child) {
      border-bottom: none;
    }
  }
</style>
