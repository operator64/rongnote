<script lang="ts">
  import { api } from '$lib/api';
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';

  type Props = { onClose: () => void };
  let { onClose }: Props = $props();

  let lat = $state(String(dashboardSettings.s.geo?.lat ?? ''));
  let lon = $state(String(dashboardSettings.s.geo?.lon ?? ''));
  let place = $state(dashboardSettings.s.geo?.place ?? '');
  let stop1 = $state(dashboardSettings.s.stop_ids[0] ?? '');
  let stop1Label = $state(dashboardSettings.s.stop_labels[0] ?? '');
  let stop2 = $state(dashboardSettings.s.stop_ids[1] ?? '');
  let stop2Label = $state(dashboardSettings.s.stop_labels[1] ?? '');

  let geoBusy = $state(false);
  let geoErr = $state('');
  async function useMyLocation() {
    if (typeof navigator === 'undefined' || !navigator.geolocation) {
      geoErr = 'browser hat keine geolocation API';
      return;
    }
    geoBusy = true;
    geoErr = '';
    try {
      const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
        navigator.geolocation.getCurrentPosition(resolve, reject, {
          enableHighAccuracy: false,
          maximumAge: 5 * 60 * 1000,
          timeout: 15_000
        });
      });
      lat = pos.coords.latitude.toFixed(4);
      lon = pos.coords.longitude.toFixed(4);
      // Reverse-geocode via open-meteo's free endpoint.
      try {
        const r = await fetch(
          `https://geocoding-api.open-meteo.com/v1/reverse?latitude=${lat}&longitude=${lon}&language=de`
        );
        if (r.ok) {
          const j = (await r.json()) as { results?: { name: string }[] };
          place = j.results?.[0]?.name ?? '';
        }
      } catch {
        /* leave place empty if reverse-geocode fails */
      }
    } catch (err) {
      const e = err as GeolocationPositionError | Error;
      geoErr = 'message' in e ? e.message : 'standort verweigert';
    } finally {
      geoBusy = false;
    }
  }

  async function autoNearestStop() {
    if (!lat || !lon) {
      geoErr = 'erst standort setzen';
      return;
    }
    geoBusy = true;
    geoErr = '';
    try {
      const stops = await api.transitNearby(parseFloat(lat), parseFloat(lon), 2);
      if (stops[0]) {
        stop1 = stops[0].id;
        stop1Label = stops[0].name;
      }
      if (stops[1]) {
        stop2 = stops[1].id;
        stop2Label = stops[1].name;
      }
    } catch (err) {
      geoErr = err instanceof Error ? err.message : 'lookup failed';
    } finally {
      geoBusy = false;
    }
  }

  function save() {
    const latN = parseFloat(lat);
    const lonN = parseFloat(lon);
    const geo =
      Number.isFinite(latN) && Number.isFinite(lonN)
        ? { lat: latN, lon: lonN, place }
        : null;
    const stop_ids: string[] = [];
    const stop_labels: string[] = [];
    if (stop1.trim()) {
      stop_ids.push(stop1.trim());
      stop_labels.push(stop1Label.trim() || stop1.trim());
    }
    if (stop2.trim()) {
      stop_ids.push(stop2.trim());
      stop_labels.push(stop2Label.trim() || stop2.trim());
    }
    dashboardSettings.save({ geo, stop_ids, stop_labels });
    onClose();
  }
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
      <strong>dashboard einstellungen</strong>
      <span class="grow"></span>
      <button type="button" onclick={onClose}>schließen</button>
    </div>
    <div class="body">

      <h3>standort (für wetter + öpnv)</h3>
      <div class="row">
        <button type="button" onclick={useMyLocation} disabled={geoBusy}>
          {geoBusy ? '…' : 'GPS verwenden'}
        </button>
        <span class="muted small">oder lat/lon manuell eintragen:</span>
      </div>
      <div class="row">
        <label class="field">
          <span class="lbl">lat</span>
          <input type="number" step="0.0001" bind:value={lat} placeholder="51.2277" />
        </label>
        <label class="field">
          <span class="lbl">lon</span>
          <input type="number" step="0.0001" bind:value={lon} placeholder="6.7735" />
        </label>
        <label class="field grow">
          <span class="lbl">ort</span>
          <input type="text" bind:value={place} placeholder="Düsseldorf" />
        </label>
      </div>
      {#if geoErr}<div class="danger small">{geoErr}</div>{/if}

      <h3>öpnv-haltestellen (VRR IDs, max 2)</h3>
      <div class="row">
        <button
          type="button"
          onclick={autoNearestStop}
          disabled={geoBusy || !lat || !lon}
          title="findet die zwei nächsten haltestellen"
        >zwei nächste finden</button>
        <span class="muted small">oder VRR-Stop-IDs (z.B. <code>20018235</code> für Düsseldorf Hbf):</span>
      </div>
      <div class="row">
        <label class="field grow">
          <span class="lbl">stop 1</span>
          <input type="text" bind:value={stop1} placeholder="20018235" />
        </label>
        <label class="field grow">
          <span class="lbl">label</span>
          <input type="text" bind:value={stop1Label} placeholder="Düsseldorf Hbf" />
        </label>
      </div>
      <div class="row">
        <label class="field grow">
          <span class="lbl">stop 2</span>
          <input type="text" bind:value={stop2} placeholder="(optional)" />
        </label>
        <label class="field grow">
          <span class="lbl">label</span>
          <input type="text" bind:value={stop2Label} />
        </label>
      </div>

      <p class="muted small" style="margin-top: 16px;">
        Liste fürs lists-widget wird im widget-header gewählt (dropdown) — alle
        deine pinned lists tauchen dort auf.
      </p>
    </div>
    <div class="foot">
      <button type="button" onclick={onClose}>cancel</button>
      <button type="button" class="primary" onclick={save}>save</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 100;
    display: flex; justify-content: center; align-items: flex-start;
    padding-top: 8vh;
  }
  .modal {
    width: min(560px, 92vw);
    max-height: 84vh;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex; flex-direction: column;
  }
  .head, .foot {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .foot { border-bottom: none; border-top: 1px solid var(--border); justify-content: flex-end; }
  .grow { flex: 1; }
  .body { padding: 12px 14px; overflow-y: auto; flex: 1; }
  h3 {
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--muted); margin: 16px 0 8px; font-weight: 500;
    border-top: 1px solid var(--border); padding-top: 12px;
  }
  h3:first-of-type { border-top: none; padding-top: 0; margin-top: 0; }
  .row { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; flex-wrap: wrap; }
  .field { display: flex; flex-direction: column; gap: 2px; }
  .field .lbl { color: var(--muted); font-size: 11px; }
  .field input {
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 3px 8px;
  }
  .field input[type="number"] { width: 100px; }
  .field.grow { flex: 1; min-width: 120px; }
  .field.grow input { width: 100%; }
  .small { font-size: 11px; }
  code {
    background: rgba(127, 127, 127, 0.10);
    padding: 0 4px;
  }
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
</style>
