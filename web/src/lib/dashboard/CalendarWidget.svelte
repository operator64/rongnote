<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import Widget from './Widget.svelte';

  /// Today + tomorrow events, derived from items.list (which already
  /// contains the union when the calendar route or any space switcher
  /// has hydrated it). Updates live as items.list mutates.
  ///
  /// Has an inline + button that opens a small modal: this is the only
  /// way a kiosk-only user (no /items chrome, no Cmd-K) can post a new
  /// event without leaving the dashboard.

  function startOfToday(): Date {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d;
  }
  function startOfTomorrow(): Date {
    const d = startOfToday();
    d.setDate(d.getDate() + 1);
    return d;
  }
  function startOfDayAfterTomorrow(): Date {
    const d = startOfToday();
    d.setDate(d.getDate() + 2);
    return d;
  }

  let todayMs = $derived(startOfToday().getTime());
  let tomorrowMs = $derived(startOfTomorrow().getTime());
  let dayAfterMs = $derived(startOfDayAfterTomorrow().getTime());

  let buckets = $derived.by(() => {
    const today: typeof items.list = [];
    const tomorrow: typeof items.list = [];
    for (const it of items.list) {
      if (it.type !== 'event' || !it.start_at) continue;
      const t = new Date(it.start_at).getTime();
      if (t >= todayMs && t < tomorrowMs) today.push(it);
      else if (t >= tomorrowMs && t < dayAfterMs) tomorrow.push(it);
    }
    today.sort((a, b) => (a.start_at ?? '').localeCompare(b.start_at ?? ''));
    tomorrow.sort((a, b) => (a.start_at ?? '').localeCompare(b.start_at ?? ''));
    return { today, tomorrow };
  });

  function timeOf(it: { start_at?: string | null; all_day?: boolean }): string {
    if (it.all_day) return 'ganztägig';
    if (!it.start_at) return '';
    return new Date(it.start_at).toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit'
    });
  }

  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(id);
  });
  function isLive(it: { start_at?: string | null; end_at?: string | null }): boolean {
    if (!it.start_at) return false;
    const s = new Date(it.start_at).getTime();
    const e = it.end_at ? new Date(it.end_at).getTime() : s + 60 * 60 * 1000;
    return s <= now.getTime() && now.getTime() < e;
  }

  // --- Inline event-create modal ---
  let modalOpen = $state(false);
  let nTitle = $state('');
  let nDate = $state('');
  let nStart = $state('09:00');
  let nEnd = $state('10:00');
  let nAllDay = $state(false);
  let saving = $state(false);
  let saveError = $state('');

  function todayYmd(): string {
    const d = new Date();
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  function openCreate() {
    nTitle = '';
    nDate = todayYmd();
    nAllDay = false;
    // round to next half-hour
    const now = new Date();
    const m = now.getMinutes() < 30 ? 30 : 0;
    const h = now.getMinutes() < 30 ? now.getHours() : (now.getHours() + 1) % 24;
    nStart = `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
    nEnd = `${String((h + 1) % 24).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
    saveError = '';
    modalOpen = true;
  }

  async function save() {
    if (!nTitle.trim()) {
      saveError = 'titel fehlt';
      return;
    }
    if (!nDate) {
      saveError = 'datum fehlt';
      return;
    }
    saving = true;
    saveError = '';
    try {
      let start_at: string;
      let end_at: string;
      if (nAllDay) {
        // Store as midnight UTC of the start day, end as next-midnight
        // (iCal DTEND exclusive convention — same as EventEditor).
        const d = new Date(`${nDate}T00:00:00Z`);
        start_at = d.toISOString();
        const next = new Date(d);
        next.setUTCDate(next.getUTCDate() + 1);
        end_at = next.toISOString();
      } else {
        const s = new Date(`${nDate}T${nStart}:00`);
        const e = new Date(`${nDate}T${nEnd}:00`);
        if (Number.isNaN(s.getTime()) || Number.isNaN(e.getTime())) {
          throw new Error('ungültige uhrzeit');
        }
        if (e <= s) throw new Error('ende muss nach dem start liegen');
        start_at = s.toISOString();
        end_at = e.toISOString();
      }
      const space_id = spaces.active?.id;
      const item = await api.createItem({
        title: nTitle.trim(),
        type: 'event',
        start_at,
        end_at,
        all_day: nAllDay,
        space_id
      });
      items.upsert(item);
      modalOpen = false;
    } catch (err) {
      saveError = err instanceof Error ? err.message : 'speichern fehlgeschlagen';
    } finally {
      saving = false;
    }
  }
</script>

<Widget title="kalender" meta="heute · morgen">
  {#snippet actions()}
    <button type="button" onclick={openCreate} title="neues event">+</button>
    <button type="button" onclick={() => goto('/items/calendar')} title="open calendar">↗</button>
  {/snippet}

  {#if buckets.today.length === 0 && buckets.tomorrow.length === 0}
    <div class="muted empty">nichts geplant.</div>
  {:else}
    {#if buckets.today.length > 0}
      <div class="day-label">heute</div>
      {#each buckets.today as ev (ev.id)}
        <a class="event" class:live={isLive(ev)} href={`/items/${ev.id}`}>
          <span class="when">{timeOf(ev)}</span>
          <span class="what">{ev.title}</span>
        </a>
      {/each}
    {/if}
    {#if buckets.tomorrow.length > 0}
      <div class="day-label" style="margin-top: 10px;">morgen</div>
      {#each buckets.tomorrow as ev (ev.id)}
        <a class="event" href={`/items/${ev.id}`}>
          <span class="when">{timeOf(ev)}</span>
          <span class="what">{ev.title}</span>
        </a>
      {/each}
    {/if}
  {/if}
</Widget>

{#if modalOpen}
  <div
    class="modal-overlay"
    role="presentation"
    onclick={() => (modalOpen = false)}
    onkeydown={(e) => e.key === 'Escape' && (modalOpen = false)}
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
        <strong>neues event</strong>
        <span class="grow"></span>
        <button type="button" onclick={() => (modalOpen = false)}>schließen</button>
      </div>
      <div class="body">
        <label class="field">
          <span class="lbl">titel</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" bind:value={nTitle} placeholder="zahnarzt, müll raus, ..." autofocus />
        </label>
        <label class="row-toggle">
          <input type="checkbox" bind:checked={nAllDay} />
          <span>ganztägig</span>
        </label>
        <div class="row">
          <label class="field">
            <span class="lbl">datum</span>
            <input type="date" bind:value={nDate} />
          </label>
          {#if !nAllDay}
            <label class="field">
              <span class="lbl">von</span>
              <input type="time" bind:value={nStart} />
            </label>
            <label class="field">
              <span class="lbl">bis</span>
              <input type="time" bind:value={nEnd} />
            </label>
          {/if}
        </div>
        {#if saveError}<div class="danger small">{saveError}</div>{/if}
      </div>
      <div class="foot">
        <button type="button" onclick={() => (modalOpen = false)}>cancel</button>
        <button type="button" class="primary" onclick={save} disabled={saving}>
          {saving ? 'speichern…' : 'save'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .day-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 4px;
  }
  .event {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    color: var(--fg);
    text-decoration: none;
  }
  .event:hover { text-decoration: none; background: rgba(127, 127, 127, 0.06); }
  .event:last-child { border-bottom: none; }
  .event.live { color: var(--accent); font-weight: 600; }
  .event .when {
    flex-shrink: 0;
    width: 70px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .event.live .when { color: var(--accent); }
  .event .what {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    padding: 16px 0;
    text-align: center;
    font-size: 12px;
    color: var(--muted);
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 100;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 10vh;
  }
  .modal {
    width: min(420px, 92vw);
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
  .body { padding: 12px 14px; }
  .field { display: flex; flex-direction: column; gap: 2px; margin-bottom: 8px; }
  .field .lbl { color: var(--muted); font-size: 11px; }
  .field input {
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 4px 8px;
    font: inherit;
  }
  .row { display: flex; gap: 8px; align-items: flex-end; }
  .row .field { flex: 1; margin-bottom: 0; }
  .row-toggle {
    display: flex; align-items: center; gap: 6px;
    margin-bottom: 8px;
    cursor: pointer;
  }
  .small { font-size: 11px; }
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .head button, .foot button, .modal button {
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 4px 10px;
    cursor: pointer;
    font: inherit;
  }
</style>
