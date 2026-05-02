<script lang="ts">
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import Widget from './Widget.svelte';

  /// Public transport departures via db-rest (https://v6.db.transport.rest).
  /// Free, OSM-funded, DB-data-backed. Up to 2 stops side-by-side. The
  /// settings modal handles GPS-based "find nearest" + manual stop IDs.

  type Departure = {
    tripId: string;
    when: string | null;
    plannedWhen: string | null;
    delay: number | null;
    direction: string;
    line: { name: string; productName?: string; product?: string };
    cancelled?: boolean;
  };

  type StopState = {
    id: string;
    label: string;
    deps: Departure[];
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

  /// Fetch one stop's departures with one retry on 5xx (db-rest's upstream
  /// HAFAS proxy throws transient 500s several times an hour). Linkbox
  /// shows the previous cached departures meanwhile.
  async function fetchOnce(id: string): Promise<Departure[]> {
    const url =
      `https://v6.db.transport.rest/stops/${encodeURIComponent(id)}/departures` +
      `?duration=60&results=8`;
    const res = await fetch(url);
    if (!res.ok) {
      const body = await res.text().catch(() => '');
      const tag = res.status >= 500 ? 'db down' : `db-rest ${res.status}`;
      throw new Error(body ? `${tag}: ${body.slice(0, 80)}` : tag);
    }
    const json = (await res.json()) as { departures: Departure[] };
    return json.departures ?? [];
  }

  async function fetchWithRetry(id: string): Promise<Departure[]> {
    try {
      return await fetchOnce(id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : '';
      // Only retry transient upstream errors. 4xx is a config problem
      // (bad stop id) — retrying won't help.
      if (msg.startsWith('db down') || msg.includes('NetworkError')) {
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

  function lineColor(name: string, product?: string): string {
    const n = name.toLowerCase();
    if (n.startsWith('s ')) return '#006e34';
    if (n.startsWith('u ') || n.startsWith('u-')) return '#c50e1f';
    if (n.startsWith('ic ') || n.startsWith('ec ')) return '#005c8c';
    if (n.startsWith('ice')) return '#1f1f1f';
    if (n.startsWith('re ') || n.startsWith('rb ')) return '#a52429';
    if (product === 'bus' || n.startsWith('bus')) return '#5e2e8b';
    if (product === 'tram') return '#1a7f37';
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
      {#each stops as s (s.id)}
        <div class="stop">
          <div class="stop-head">{s.label}</div>
          {#if s.error && s.deps.length === 0}
            <div class="danger small">{s.error}</div>
          {:else if s.loading && s.deps.length === 0}
            <div class="muted small">…</div>
          {:else if s.deps.length === 0}
            <div class="muted small">keine abfahrten in 60 min.</div>
          {:else}
            {#if s.error}
              <div class="warn small" title={s.error}>⚠ db down — letzter erfolgreicher pull</div>
            {/if}
            {#each s.deps.slice(0, 6) as d (d.tripId)}
              {@const m = minutesFromNow(d.when ?? d.plannedWhen)}
              <div class="dep" class:cancel={d.cancelled}>
                <span class="line" style={`background: ${lineColor(d.line.name, d.line.product)};`}>
                  {d.line.name}
                </span>
                <span class="dest" title={d.direction}>{d.direction}</span>
                <span
                  class="min"
                  class:now={m !== null && m <= 1}
                  class:late={d.delay !== null && d.delay >= 60}
                >
                  {#if d.cancelled}—{:else if m === null}?{:else if m <= 0}jetzt{:else}{m}{/if}
                </span>
              </div>
            {/each}
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    min-width: 36px;
    text-align: right;
  }
  .min.now { color: var(--accent); font-weight: 600; }
  .min.late { color: var(--warn, #d18616); }
  .small { font-size: 12px; }
  .warn {
    color: var(--warn, #d18616);
    padding: 2px 4px;
    border: 1px dashed var(--warn, #d18616);
    margin-bottom: 4px;
    font-size: 10px;
  }
</style>
