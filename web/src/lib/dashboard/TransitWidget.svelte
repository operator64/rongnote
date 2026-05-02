<script lang="ts">
  import { api, ApiError, type TransitDeparture } from '$lib/api';
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import Widget from './Widget.svelte';

  /// Public transport departures via our /api/v1/transit proxy, which
  /// fetches VRR EFA (https://efa.vrr.de) on the server. Direct browser
  /// fetches against EFA are blocked by CORS, so the proxy is what makes
  /// this work without a third-party intermediary. Up to 2 stops side
  /// by side. Settings modal does GPS-based "find nearest" + manual IDs.

  type StopState = {
    id: string;
    label: string;
    deps: TransitDeparture[];
    error: string;
    loading: boolean;
  };

  let stops = $state<StopState[]>([]);
  let lastRefresh = $state<Date | null>(null);

  $effect(() => {
    void dashboardSettings.s.stop_ids;
    void dashboardSettings.s.stop_labels;
    void refresh();
    const id = setInterval(refresh, 60_000);
    return () => clearInterval(id);
  });

  /// Fetch a bigger window than we render — the walk-time filter and
  /// occasional cancellations would otherwise empty the visible list.
  const FETCH_LIMIT = 20;
  /// Cap on how many reachable departures we show per stop.
  const SHOW_LIMIT = 10;

  /// One retry on 502 — that's our server's "vrr efa unavailable" status,
  /// usually a transient upstream wobble. 4xx (bad stop id) shouldn't
  /// retry. Stale departures stay visible meanwhile via the renderer.
  async function fetchOnce(id: string): Promise<TransitDeparture[]> {
    return await api.transitDepartures(id, FETCH_LIMIT);
  }

  async function fetchWithRetry(id: string): Promise<TransitDeparture[]> {
    try {
      return await fetchOnce(id);
    } catch (err) {
      const isUpstreamWobble =
        err instanceof ApiError && err.status >= 500;
      const isNetwork = !(err instanceof ApiError);
      if (isUpstreamWobble || isNetwork) {
        await new Promise((r) => setTimeout(r, 1500));
        return await fetchOnce(id);
      }
      throw err;
    }
  }

  async function refresh() {
    const ids = dashboardSettings.s.stop_ids.slice(0, 2);
    const labels = dashboardSettings.s.stop_labels;
    if (ids.length === 0) {
      stops = [];
      return;
    }
    // Initialise rows on first run only — subsequent refreshes keep the
    // previous deps visible until the new fetch resolves, so a transient
    // 5xx doesn't blank the widget.
    if (stops.length !== ids.length || stops.some((s, i) => s.id !== ids[i])) {
      stops = ids.map((id, i) => ({
        id,
        label: labels[i] ?? id,
        deps: [],
        error: '',
        loading: true
      }));
    } else {
      for (let i = 0; i < stops.length; i++) {
        stops[i].label = labels[i] ?? stops[i].id;
        stops[i].loading = true;
      }
    }
    await Promise.all(
      ids.map(async (_id, i) => {
        try {
          stops[i].deps = await fetchWithRetry(stops[i].id);
          stops[i].error = '';
        } catch (err) {
          stops[i].error = err instanceof Error ? err.message : 'fetch failed';
          // keep stops[i].deps so the previous values stay visible
        } finally {
          stops[i].loading = false;
        }
      })
    );
    stops = stops; // trigger reactivity
    lastRefresh = new Date();
  }

  function minutesFromNow(iso: string | null): number | null {
    if (!iso) return null;
    return Math.round((new Date(iso).getTime() - Date.now()) / 60_000);
  }

  function clockTime(iso: string | null): string {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  /// Server gives us a clean `product` bucket; fall back to name prefix
  /// (covers both "S 1" db-style and "S1" vrr-style) for older payloads.
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

  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (now = new Date()), 30_000);
    return () => clearInterval(id);
  });
</script>

<Widget
  title="öpnv"
  meta={lastRefresh ? lastRefresh.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }) : ''}
>
  {#snippet actions()}
    <button type="button" onclick={refresh} title="refresh">↻</button>
  {/snippet}

  {#if stops.length === 0}
    <div class="muted small">
      keine haltestellen konfiguriert. öffne die einstellungen oben rechts und
      wähle "GPS" oder trag db-rest stop-IDs ein.
    </div>
  {:else}
    <div class="cols">
      {#each stops as s, i (s.id)}
        {@const walk = dashboardSettings.s.walk_minutes[i] ?? 0}
        {@const visible = s.deps
          .filter((d) => {
            if (d.cancelled) return true;
            const m = minutesFromNow(d.when ?? d.planned_when);
            return m === null || m >= walk;
          })
          .slice(0, SHOW_LIMIT)}
        {@const hiddenCount =
          walk > 0
            ? s.deps.filter((d) => {
                if (d.cancelled) return false;
                const m = minutesFromNow(d.when ?? d.planned_when);
                return m !== null && m < walk;
              }).length
            : 0}
        <div class="stop">
          <div class="stop-head">
            <span class="stop-name">{s.label}</span>
            {#if walk > 0}
              <span class="walk" title="fußweg {walk} min — abfahrten unter dieser zeit sind ausgeblendet">
                🚶 {walk}m
              </span>
            {/if}
          </div>
          {#if s.error && s.deps.length === 0}
            <div class="danger small">{s.error}</div>
          {:else if s.loading && s.deps.length === 0}
            <div class="muted small">…</div>
          {:else if visible.length === 0 && s.deps.length > 0}
            <div class="muted small">
              keine erreichbaren abfahrten {walk > 0 ? `(fußweg ${walk} min)` : ''}.
            </div>
          {:else if s.deps.length === 0}
            <div class="muted small">keine abfahrten in 60 min.</div>
          {:else}
            {#if s.error}
              <div class="warn small" title={s.error}>⚠ vrr down — letzter erfolgreicher pull</div>
            {/if}
            {#each visible as d (d.trip_id)}
              {@const m = minutesFromNow(d.when ?? d.planned_when)}
              {@const leaveIn = m !== null ? m - walk : null}
              {@const t = clockTime(d.when ?? d.planned_when)}
              <div class="dep" class:cancel={d.cancelled}>
                <span class="line" style={`background: ${lineColor(d.line.name, d.line.product)};`}>
                  {d.line.name}
                </span>
                <span class="dest" title={d.direction}>{d.direction}</span>
                <span
                  class="min"
                  class:now={leaveIn !== null && leaveIn <= 1}
                  class:late={d.delay !== null && d.delay >= 60}
                  title={walk > 0 && m !== null ? `abfahrt um ${t}, in ${m} min — los in ${leaveIn} min` : (m !== null ? `abfahrt um ${t}, in ${m} min` : '')}
                >
                  <span class="time">{t}</span>
                  <span class="cd">
                    {#if d.cancelled}—{:else if m === null}?{:else if leaveIn !== null && leaveIn <= 0}los{:else}{leaveIn ?? m}m{/if}
                  </span>
                </span>
              </div>
            {/each}
            {#if hiddenCount > 0}
              <div class="muted small foot">{hiddenCount} weiter unter fußweg ausgeblendet</div>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</Widget>

<style>
  .cols {
    display: flex;
    gap: 0;
    height: 100%;
    min-height: 0;
  }
  .stop {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    padding-right: 8px;
    margin-right: 8px;
    overflow-y: auto;
  }
  .stop:last-child {
    border-right: none;
    margin-right: 0;
    padding-right: 0;
  }
  .stop-head {
    font-size: 11px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 4px;
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-shrink: 0;
  }
  .stop-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
    min-width: 0;
  }
  .walk {
    text-transform: none;
    letter-spacing: 0;
    color: var(--accent);
    font-size: 10px;
    flex-shrink: 0;
  }
  .foot {
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }
  .dep {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
  }
  .dep:last-child { border-bottom: none; }
  .dep.cancel { opacity: 0.5; text-decoration: line-through; }
  .line {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 32px;
    max-width: 56px;
    height: 18px;
    color: white;
    font-size: 11px;
    font-weight: 600;
    padding: 0 5px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dest {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .min {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    font-size: 12px;
    min-width: 78px;
    text-align: right;
    display: inline-flex;
    justify-content: flex-end;
    align-items: baseline;
    gap: 6px;
  }
  .min .time {
    font-size: 12px;
  }
  .min .cd {
    font-size: 11px;
    min-width: 28px;
    text-align: right;
  }
  .min.now .cd { color: var(--accent); font-weight: 600; }
  .min.late .cd { color: var(--warn, #d18616); }
  .small { font-size: 12px; }
  .warn {
    color: var(--warn, #d18616);
    padding: 2px 4px;
    border: 1px dashed var(--warn, #d18616);
    margin-bottom: 4px;
    font-size: 10px;
  }
</style>
