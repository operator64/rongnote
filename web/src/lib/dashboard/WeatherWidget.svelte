<script lang="ts">
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import Widget from './Widget.svelte';

  /// open-meteo.com — free, no API key. Current temp + 4-day forecast.
  /// Compact layout: hero on the left (icon + temp + place), forecast
  /// rows next to it. No empty space at the bottom.

  type Hourly = {
    current?: {
      temperature_2m: number;
      apparent_temperature: number;
      relative_humidity_2m: number;
      wind_speed_10m: number;
      precipitation: number;
      weather_code: number;
      is_day: number;
    };
    daily?: {
      time: string[];
      temperature_2m_max: number[];
      temperature_2m_min: number[];
      precipitation_sum: number[];
      weather_code: number[];
    };
  };

  let data = $state<Hourly | null>(null);
  let loading = $state(false);
  let error = $state('');
  let lastFetch = $state(0);

  /// Refresh every 10 minutes. Only fetches when geo is set; the
  /// dashboard settings modal handles GPS lookup.
  $effect(() => {
    void dashboardSettings.s.geo;
    void refresh();
    const id = setInterval(refresh, 10 * 60 * 1000);
    return () => clearInterval(id);
  });

  async function refresh() {
    const geo = dashboardSettings.s.geo;
    if (!geo) {
      data = null;
      error = '';
      return;
    }
    if (Date.now() - lastFetch < 30_000) return; // throttle
    loading = true;
    error = '';
    try {
      const url =
        `https://api.open-meteo.com/v1/forecast?latitude=${geo.lat}&longitude=${geo.lon}` +
        `&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,precipitation,weather_code,is_day` +
        `&daily=temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code` +
        `&timezone=auto&forecast_days=4`;
      const res = await fetch(url);
      if (!res.ok) throw new Error(`open-meteo ${res.status}`);
      data = await res.json();
      lastFetch = Date.now();
    } catch (err) {
      error = err instanceof Error ? err.message : 'fetch failed';
    } finally {
      loading = false;
    }
  }

  // WMO weather codes → emoji + label
  function wmo(code: number, day = true): { icon: string; label: string } {
    if (code === 0) return { icon: day ? '☀️' : '🌙', label: 'klar' };
    if (code === 1) return { icon: day ? '🌤' : '🌙', label: 'überw. klar' };
    if (code === 2) return { icon: day ? '⛅' : '☁️', label: 'teilw. bewölkt' };
    if (code === 3) return { icon: '☁️', label: 'bedeckt' };
    if (code === 45 || code === 48) return { icon: '🌫', label: 'nebel' };
    if (code >= 51 && code <= 57) return { icon: '🌦', label: 'nieselregen' };
    if (code >= 61 && code <= 67) return { icon: '🌧', label: 'regen' };
    if (code >= 71 && code <= 77) return { icon: '🌨', label: 'schnee' };
    if (code >= 80 && code <= 82) return { icon: '🌧', label: 'schauer' };
    if (code >= 85 && code <= 86) return { icon: '🌨', label: 'schneeschauer' };
    if (code >= 95) return { icon: '⛈', label: 'gewitter' };
    return { icon: '·', label: '' };
  }

  function dayLabel(iso: string, idx: number): string {
    if (idx === 0) return 'heute';
    if (idx === 1) return 'morgen';
    return new Date(iso).toLocaleDateString(undefined, { weekday: 'short' });
  }

  let geoLabel = $derived(dashboardSettings.s.geo?.place ?? '');
</script>

<Widget title={`wetter${geoLabel ? ' · ' + geoLabel : ''}`} meta={loading ? '…' : ''}>
  {#snippet actions()}
    <button type="button" onclick={refresh} title="refresh">↻</button>
  {/snippet}

  {#if !dashboardSettings.s.geo}
    <div class="muted small">
      kein standort konfiguriert. öffne die einstellungen oben rechts und wähle "GPS"
      oder trag lat/lon ein.
    </div>
  {:else if error}
    <div class="danger small">{error}</div>
  {:else if !data?.current}
    <div class="muted small">…</div>
  {:else}
    {@const c = wmo(data.current.weather_code, !!data.current.is_day)}
    <div class="now-row">
      <span class="icon">{c.icon}</span>
      <span class="temp">{Math.round(data.current.temperature_2m)}°</span>
      <span class="cond">{c.label}</span>
      <span class="grow"></span>
      <span class="kv"><span class="k">gefühlt</span><span>{Math.round(data.current.apparent_temperature)}°</span></span>
      <span class="kv"><span class="k">wind</span><span>{Math.round(data.current.wind_speed_10m)} km/h</span></span>
    </div>
    {#if data.daily}
      <div class="forecast">
        {#each data.daily.time as t, i (t)}
          {@const d = wmo(data.daily.weather_code[i])}
          <div class="day">
            <span class="lbl">{dayLabel(t, i)}</span>
            <span class="ic">{d.icon}</span>
            <span class="hi">{Math.round(data.daily.temperature_2m_max[i])}°</span>
            <span class="lo muted">{Math.round(data.daily.temperature_2m_min[i])}°</span>
            {#if data.daily.precipitation_sum[i] > 0.5}
              <span class="rain muted">{data.daily.precipitation_sum[i].toFixed(1)} mm</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</Widget>

<style>
  .now-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 6px;
    flex-wrap: wrap;
  }
  .now-row .icon { font-size: 22px; line-height: 1; }
  .now-row .temp { font-size: 22px; font-weight: 600; line-height: 1; }
  .now-row .cond { color: var(--muted); font-size: 12px; }
  .grow { flex: 1; }
  .kv {
    display: inline-flex; gap: 4px; align-items: baseline;
    font-size: 11px; color: var(--muted);
  }
  .kv .k { text-transform: uppercase; letter-spacing: 0.05em; }
  .forecast {
    display: flex; flex-direction: column;
    gap: 0;
  }
  .day {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 0;
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }
  .day:last-child { border-bottom: none; }
  .day .lbl { width: 60px; color: var(--muted); flex-shrink: 0; }
  .day .ic { width: 20px; flex-shrink: 0; }
  .day .hi { font-weight: 600; min-width: 32px; }
  .day .lo { min-width: 32px; }
  .day .rain { font-size: 11px; }
  .small { font-size: 12px; }
</style>
