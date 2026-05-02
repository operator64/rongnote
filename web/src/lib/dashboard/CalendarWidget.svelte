<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import Widget from './Widget.svelte';

  /// Week-strip + agenda layout (matches the dashboard-grid prototype).
  /// Top row = ISO week (Mo–So) with today highlighted, dot under any
  /// day that has events. Body = chronological list of events for the
  /// currently-selected day. Tap a day to switch.
  ///
  /// The "+event" modal is unchanged from the previous version — kiosk
  /// users still need a way to post events without ever seeing /items.

  // --- Date helpers -----------------------------------------------------
  function startOfDay(d: Date): Date {
    const r = new Date(d);
    r.setHours(0, 0, 0, 0);
    return r;
  }
  function ymd(d: Date): string {
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  /// ISO 8601 week number (Mon-start, week with Jan 4th in it).
  function isoWeek(date: Date): number {
    const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
    const dayNum = d.getUTCDay() || 7;
    d.setUTCDate(d.getUTCDate() + 4 - dayNum);
    const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
    return Math.ceil(((d.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7);
  }
  function mondayOf(d: Date): Date {
    const r = startOfDay(d);
    const dow = (r.getDay() + 6) % 7; // 0 = Mon
    r.setDate(r.getDate() - dow);
    return r;
  }

  // --- Live "now" tick (for today highlight + isLive) -------------------
  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(id);
  });
  let todayYmd = $derived(ymd(now));

  // --- Selection + week ------------------------------------------------
  let selected = $state<Date>(startOfDay(new Date()));
  let weekStart = $derived(mondayOf(selected));
  let weekDays = $derived.by<Date[]>(() => {
    const out: Date[] = [];
    for (let i = 0; i < 7; i++) {
      const d = new Date(weekStart);
      d.setDate(d.getDate() + i);
      out.push(d);
    }
    return out;
  });

  // Map ymd → events for that day, sorted by start_at.
  let eventsByDay = $derived.by<Map<string, typeof items.list>>(() => {
    const map = new Map<string, typeof items.list>();
    for (const it of items.list) {
      if (it.type !== 'event' || !it.start_at) continue;
      const key = ymd(new Date(it.start_at));
      const arr = map.get(key) ?? [];
      arr.push(it);
      map.set(key, arr);
    }
    for (const arr of map.values()) {
      arr.sort((a, b) => (a.start_at ?? '').localeCompare(b.start_at ?? ''));
    }
    return map;
  });

  let selectedYmd = $derived(ymd(selected));
  let selectedEvents = $derived(eventsByDay.get(selectedYmd) ?? []);

  function isToday(d: Date): boolean {
    return ymd(d) === todayYmd;
  }
  function isSelected(d: Date): boolean {
    return ymd(d) === selectedYmd;
  }

  function timeOf(it: { start_at?: string | null; all_day?: boolean }): string {
    if (it.all_day) return 'ganztägig';
    if (!it.start_at) return '';
    return new Date(it.start_at).toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit'
    });
  }
  function isLive(it: { start_at?: string | null; end_at?: string | null }): boolean {
    if (!it.start_at) return false;
    const s = new Date(it.start_at).getTime();
    const e = it.end_at ? new Date(it.end_at).getTime() : s + 60 * 60 * 1000;
    return s <= now.getTime() && now.getTime() < e;
  }

  // Meta line: "KW 18 · Sa, 2. Mai" — the KW number tracks the *selected*
  // day so the user can navigate weeks visually if we add prev/next later.
  let meta = $derived.by(() => {
    const dayLabel = selected.toLocaleDateString(undefined, {
      weekday: 'short',
      day: 'numeric',
      month: 'short'
    });
    return `KW ${isoWeek(selected)} · ${dayLabel}`;
  });

  // --- "+ event" modal ---------------------------------------------------
  let modalOpen = $state(false);
  let nTitle = $state('');
  let nDate = $state('');
  let nStart = $state('09:00');
  let nEnd = $state('10:00');
  let nAllDay = $state(false);
  let saving = $state(false);
  let saveError = $state('');

  function openCreate() {
    nTitle = '';
    nDate = selectedYmd; // default to whichever day the user is looking at
    nAllDay = false;
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

  const WEEKDAY_LABELS = ['Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa', 'So'];
</script>

<Widget title="kalender" {meta}>
  {#snippet actions()}
    <button type="button" onclick={openCreate} title="neues event">+</button>
    <button type="button" onclick={() => goto('/items/calendar')} title="open calendar">↗</button>
  {/snippet}

  <div class="week-strip">
    {#each weekDays as d, i (d.getTime())}
      {@const has = (eventsByDay.get(ymd(d))?.length ?? 0) > 0}
      <button
        type="button"
        class="day"
        class:today={isToday(d)}
        class:selected={isSelected(d)}
        onclick={() => (selected = startOfDay(d))}
      >
        <span class="dow">{WEEKDAY_LABELS[i]}</span>
        <span class="num">{d.getDate()}</span>
        <span class="dot" class:visible={has}></span>
      </button>
    {/each}
  </div>

  {#if selectedEvents.length === 0}
    <div class="muted empty">
      {isToday(selected) ? 'heute' : selected.toLocaleDateString(undefined, { weekday: 'long' })} —
      nichts geplant.
    </div>
  {:else}
    {#each selectedEvents as ev (ev.id)}
      <a class="event" class:live={isLive(ev)} href={`/items/${ev.id}`}>
        <span class="when">{timeOf(ev)}</span>
        <span class="what">{ev.title}</span>
      </a>
    {/each}
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
  /* The week strip extends edge-to-edge, undoing the widget body's
     horizontal padding so the day cells align with the widget head. */
  .week-strip {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    margin: -10px -12px 8px;
    border-bottom: 1px solid var(--border);
  }
  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    padding: 6px 0 4px;
    background: transparent;
    border: none;
    border-right: 1px solid var(--border);
    color: var(--fg);
    cursor: pointer;
    font: inherit;
  }
  .day:last-child { border-right: none; }
  .day:hover { background: rgba(127, 127, 127, 0.08); }
  .day .dow { font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .day .num { font-size: 16px; line-height: 1; margin-top: 1px; }
  .day .dot {
    width: 4px; height: 4px;
    margin-top: 3px;
    border-radius: 50%;
    background: transparent;
  }
  .day .dot.visible { background: var(--accent); }
  .day.today { background: rgba(127, 127, 127, 0.08); }
  .day.today .num { color: var(--accent); font-weight: 600; }
  .day.selected { background: rgba(127, 127, 127, 0.18); }
  .day.selected .num { font-weight: 600; }
  .day.today.selected { background: rgba(127, 127, 127, 0.22); }

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
