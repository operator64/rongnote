<script lang="ts">
  import { api, ApiError, type TransitDeparture } from '$lib/api';

  /// Big-text modal for a single stop. Opens when the kiosk taps the
  /// stop column in TransitWidget. Auto-refreshes while open so the
  /// modal stays fresh; closes via overlay click or × button.
  ///
  /// Walk-time (passed in by the parent) still hides departures the
  /// rider can't catch, same rule as the embedded widget.

  type Props = {
    stopId: string;
    label: string;
    walkMinutes: number;
    onClose: () => void;
  };
  let { stopId, label, walkMinutes, onClose }: Props = $props();

  const FETCH_LIMIT = 30;
  const SHOW_LIMIT = 24;

  let deps = $state<TransitDeparture[]>([]);
  let loading = $state(true);
  let error = $state('');
  let lastRefresh = $state<Date | null>(null);

  $effect(() => {
    void stopId;
    void load();
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  });

  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (now = new Date()), 15_000);
    return () => clearInterval(id);
  });

  async function load() {
    try {
      deps = await api.transitDepartures(stopId, FETCH_LIMIT);
      error = '';
      lastRefresh = new Date();
    } catch (err) {
      const msg = err instanceof ApiError && err.status >= 500 ? 'vrr down' : 'fetch failed';
      error = err instanceof Error ? `${msg}: ${err.message}` : msg;
    } finally {
      loading = false;
    }
  }

  function minutesFromNow(iso: string | null): number | null {
    if (!iso) return null;
    return Math.round((new Date(iso).getTime() - now.getTime()) / 60_000);
  }
  function clockTime(iso: string | null): string {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  function lineColor(name: string, product?: string): string {
    switch (product) {
      case 'suburban':     return '#006e34';
      case 'subway':       return '#c50e1f';
      case 'tram':         return '#1a7f37';
      case 'bus':          return '#5e2e8b';
      case 'regional':     return '#a52429';
      case 'longdistance': return '#1f1f1f';
      case 'ferry':        return '#0782bf';
    }
    const n = name.toLowerCase().replace(/\s+/g, '');
    if (n.startsWith('s')) return '#006e34';
    if (n.startsWith('u')) return '#c50e1f';
    if (n.startsWith('ice')) return '#1f1f1f';
    if (n.startsWith('ic') || n.startsWith('ec')) return '#005c8c';
    if (n.startsWith('re') || n.startsWith('rb')) return '#a52429';
    return '#444';
  }

  let visible = $derived(
    deps
      .filter((d) => {
        if (d.cancelled) return true;
        const m = minutesFromNow(d.when ?? d.planned_when);
        return m === null || m >= walkMinutes;
      })
      .slice(0, SHOW_LIMIT)
  );
</script>

<div
  class="overlay"
  role="presentation"
  onclick={onClose}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="head">
      <span class="stop-label">{label}</span>
      {#if walkMinutes > 0}
        <span class="walk muted">🚶 {walkMinutes} min</span>
      {/if}
      <span class="grow"></span>
      {#if lastRefresh}
        <span class="muted small">{lastRefresh.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}</span>
      {/if}
      <button type="button" onclick={load} title="refresh">↻</button>
      <button type="button" onclick={onClose} aria-label="close">×</button>
    </div>

    <div class="body">
      {#if error && deps.length === 0}
        <div class="danger">{error}</div>
      {:else if loading && deps.length === 0}
        <div class="muted">…</div>
      {:else if visible.length === 0}
        <div class="muted">keine erreichbaren abfahrten.</div>
      {:else}
        {#if error}
          <div class="warn small">⚠ {error} — alte daten</div>
        {/if}
        {#each visible as d (d.trip_id)}
          {@const m = minutesFromNow(d.when ?? d.planned_when)}
          {@const leaveIn = m !== null ? m - walkMinutes : null}
          {@const t = clockTime(d.when ?? d.planned_when)}
          <div class="dep" class:cancel={d.cancelled}>
            <span class="line" style={`background: ${lineColor(d.line.name, d.line.product)};`}>
              {d.line.name}
            </span>
            <span class="dest" title={d.direction}>{d.direction}</span>
            <span class="time">{t}</span>
            <span
              class="cd"
              class:now={leaveIn !== null && leaveIn <= 1}
              class:late={d.delay !== null && d.delay >= 60}
            >
              {#if d.cancelled}—
              {:else if m === null}?
              {:else if leaveIn !== null && leaveIn <= 0}los
              {:else}{leaveIn ?? m}m
              {/if}
            </span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 200;
    display: flex; justify-content: center; align-items: flex-start;
    padding: 5vh 12px;
  }
  .modal {
    width: min(680px, 96vw);
    max-height: 90vh;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex; flex-direction: column;
  }
  .head {
    display: flex; align-items: baseline; gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  .stop-label { font-size: 22px; font-weight: 600; }
  .walk { font-size: 13px; color: var(--accent); }
  .grow { flex: 1; }
  .head button {
    background: transparent; color: var(--fg);
    border: 1px solid var(--border);
    padding: 4px 12px;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
  .head button:hover { border-color: var(--fg); }
  .small { font-size: 11px; }
  .body {
    padding: 8px 18px 18px;
    overflow-y: auto;
    flex: 1;
  }
  .dep {
    display: flex; align-items: center; gap: 14px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
    font-size: 17px;
  }
  .dep:last-child { border-bottom: none; }
  .dep.cancel { opacity: 0.5; text-decoration: line-through; }
  .line {
    display: inline-flex;
    align-items: center; justify-content: center;
    min-width: 50px;
    max-width: 80px;
    height: 28px;
    color: white;
    font-size: 14px;
    font-weight: 700;
    padding: 0 8px;
    flex-shrink: 0;
    border-radius: 2px;
  }
  .dest {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 17px;
  }
  .time {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    font-size: 17px;
    min-width: 60px;
    text-align: right;
  }
  .cd {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
    min-width: 56px;
    text-align: right;
    font-size: 17px;
    color: var(--muted);
  }
  .cd.now { color: var(--accent); font-weight: 700; }
  .cd.late { color: var(--warn, #d18616); }
  .warn {
    color: var(--warn, #d18616);
    padding: 6px 8px;
    border: 1px dashed var(--warn, #d18616);
    margin-bottom: 10px;
    font-size: 12px;
  }
</style>
