<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, ApiError, type ItemSummary } from '$lib/api';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';

  /// Month-grid calendar showing events from EVERY space the user is a
  /// member of (personal + each team), color-coded per space. Loads the
  /// visible 6-week window from /api/v1/items?type=event&start_after=&
  /// start_before= for each space in parallel. Re-fetches when the view
  /// window changes, the available spaces change, or any item in the
  /// vault is mutated (so editing an event in the editor and clicking
  /// "back to calendar" shows the change immediately).

  type EventWithSpace = ItemSummary & { space_id: string };

  /// Palette: personal stays accent-blue; team spaces cycle through a
  /// distinct set so events from different teams don't blend together.
  const TEAM_COLORS = [
    '#1a7f37', // green
    '#d18616', // amber
    '#6f42c1', // purple
    '#bf3989', // pink
    '#9e6a03'  // bronze
  ];
  function colorFor(spaceId: string): string {
    const sp = spaces.list.find((s) => s.id === spaceId);
    if (!sp || sp.kind === 'personal') return 'var(--accent)';
    // Order team spaces deterministically by created_at so each one keeps
    // its color across reloads.
    const teams = spaces.list
      .filter((s) => s.kind === 'team')
      .sort((a, b) => a.created_at.localeCompare(b.created_at));
    const idx = teams.findIndex((s) => s.id === spaceId);
    return TEAM_COLORS[idx % TEAM_COLORS.length];
  }
  function spaceLabel(spaceId: string): string {
    return spaces.list.find((s) => s.id === spaceId)?.name ?? '?';
  }

  let viewMonth = $state(monthAnchor(new Date()));
  let selected = $state<Date>(stripTime(new Date()));
  let events = $state<EventWithSpace[]>([]);
  let loading = $state(true);
  let error = $state('');

  /// Re-fetch on:
  /// - view-window change (← / → / heute)
  /// - spaces.list change (user joined / left / created a team space)
  /// - items.list mutation (any create/update/delete elsewhere — editor,
  ///   sidebar +, palette, …) — keeps the grid live without a manual
  ///   refresh button
  $effect(() => {
    void viewMonth;
    void spaces.list;
    void items.list;
    refresh();
  });

  async function refresh() {
    loading = true;
    error = '';
    try {
      const range = monthGridRange(viewMonth);
      const start_after = range.from.toISOString();
      const start_before = range.to.toISOString();
      const perSpace = await Promise.all(
        spaces.list.map(async (s) => {
          const list = await api.listItems({
            type: 'event',
            space_id: s.id,
            start_after,
            start_before
          });
          return list.map((it): EventWithSpace => ({ ...it, space_id: s.id }));
        })
      );
      events = perSpace.flat();
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'load failed';
    } finally {
      loading = false;
    }
  }

  function stripTime(d: Date): Date {
    const x = new Date(d);
    x.setHours(0, 0, 0, 0);
    return x;
  }
  function monthAnchor(d: Date): Date {
    return new Date(d.getFullYear(), d.getMonth(), 1);
  }
  /// 6×7 grid window starting on the Monday on or before the 1st.
  function monthGridRange(anchor: Date): { from: Date; to: Date; cells: Date[] } {
    const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
    // JS getDay: 0=Sun, 1=Mon, ..., 6=Sat. We want Monday-first.
    const offset = (first.getDay() + 6) % 7;
    const from = new Date(first);
    from.setDate(from.getDate() - offset);
    from.setHours(0, 0, 0, 0);
    const cells: Date[] = [];
    for (let i = 0; i < 42; i++) {
      const d = new Date(from);
      d.setDate(d.getDate() + i);
      cells.push(d);
    }
    const to = new Date(from);
    to.setDate(to.getDate() + 42);
    return { from, to, cells };
  }

  let grid = $derived(monthGridRange(viewMonth));

  /// Map: 'YYYY-MM-DD' → events[]. An event spans days; for the month
  /// view we attach it to the day(s) it covers within the window.
  let byDay = $derived.by(() => {
    const map = new Map<string, EventWithSpace[]>();
    for (const ev of events) {
      if (!ev.start_at) continue;
      const start = new Date(ev.start_at);
      const end = ev.end_at ? new Date(ev.end_at) : new Date(start.getTime() + 60 * 60 * 1000);
      // For all-day events, end is exclusive midnight.
      const lastDay = ev.all_day
        ? new Date(end.getTime() - 1)
        : end;
      const dStart = stripTime(start);
      const dEnd = stripTime(lastDay);
      for (
        let d = new Date(dStart);
        d.getTime() <= dEnd.getTime();
        d.setDate(d.getDate() + 1)
      ) {
        const key = ymdKey(d);
        if (!map.has(key)) map.set(key, []);
        map.get(key)!.push(ev);
      }
    }
    // Sort each day's events by start_at.
    for (const list of map.values()) {
      list.sort((a, b) => (a.start_at ?? '').localeCompare(b.start_at ?? ''));
    }
    return map;
  });

  function ymdKey(d: Date): string {
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  let today = $derived(stripTime(new Date()));
  let todayKey = $derived(ymdKey(today));
  let selectedKey = $derived(ymdKey(selected));
  let selectedEvents = $derived(byDay.get(selectedKey) ?? []);

  let monthLabel = $derived(
    viewMonth.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
  );

  function prevMonth() {
    viewMonth = new Date(viewMonth.getFullYear(), viewMonth.getMonth() - 1, 1);
  }
  function nextMonth() {
    viewMonth = new Date(viewMonth.getFullYear(), viewMonth.getMonth() + 1, 1);
  }
  function goToday() {
    viewMonth = monthAnchor(new Date());
    selected = stripTime(new Date());
  }

  function timeLabel(ev: ItemSummary): string {
    if (!ev.start_at) return '';
    if (ev.all_day) return '';
    const d = new Date(ev.start_at);
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }

  function fullTimeLabel(ev: ItemSummary): string {
    if (!ev.start_at) return '';
    if (ev.all_day) return 'ganztägig';
    const start = new Date(ev.start_at);
    const end = ev.end_at ? new Date(ev.end_at) : null;
    const fmt = (d: Date) =>
      d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
    return end ? `${fmt(start)} — ${fmt(end)}` : fmt(start);
  }

  let busy = $state(false);
  async function newEventOnSelected() {
    busy = true;
    try {
      const start = new Date(selected);
      start.setHours(9, 0, 0, 0);
      const end = new Date(start);
      end.setHours(10, 0, 0, 0);
      const created = await api.createItem({
        type: 'event',
        title: 'New event',
        start_at: start.toISOString(),
        end_at: end.toISOString(),
        all_day: false,
        space_id: spaces.activeId ?? undefined,
        path: '/'
      });
      items.upsert(created); // triggers the $effect → refresh()
      goto(`/items/${created.id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'create failed';
    } finally {
      busy = false;
    }
  }

  function selectDay(d: Date) {
    selected = stripTime(d);
  }

  function isOtherMonth(d: Date): boolean {
    return d.getMonth() !== viewMonth.getMonth();
  }
</script>

<div class="page">
  <div class="head row">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <button type="button" class="nav-btn" onclick={prevMonth}>‹</button>
    <span class="month">{monthLabel}</span>
    <button type="button" class="nav-btn" onclick={nextMonth}>›</button>
    <button type="button" onclick={goToday}>heute</button>
    {#if loading}<span class="muted small">…</span>{/if}
    {#if error}<span class="danger small">{error}</span>{/if}
    <span class="grow"></span>
    <span class="legend">
      {#each spaces.list as s (s.id)}
        <span class="legend-item" title={s.kind === 'team' ? `team · ${s.role}` : 'personal'}>
          <span class="swatch" style="background: {colorFor(s.id)};"></span>
          <span>{s.name}</span>
        </span>
      {/each}
    </span>
    <span class="muted small">{events.length} events</span>
    <button type="button" disabled={busy} onclick={newEventOnSelected}>+ event</button>
  </div>

  <div class="layout">
    <main class="month-grid">
      <div class="dow-row">
        <div class="dow">mo</div><div class="dow">di</div><div class="dow">mi</div>
        <div class="dow">do</div><div class="dow">fr</div><div class="dow">sa</div><div class="dow">so</div>
      </div>
      {#each [0, 1, 2, 3, 4, 5] as week (week)}
        <div class="week-row">
          {#each grid.cells.slice(week * 7, week * 7 + 7) as day (ymdKey(day))}
            {@const dayKey = ymdKey(day)}
            {@const dayEvents = byDay.get(dayKey) ?? []}
            <button
              type="button"
              class="day-cell"
              class:other={isOtherMonth(day)}
              class:today={dayKey === todayKey}
              class:selected={dayKey === selectedKey}
              onclick={() => selectDay(day)}
            >
              <span class="num">{day.getDate()}</span>
              {#each dayEvents.slice(0, 3) as ev (ev.id)}
                <span
                  class="pill"
                  style="background: {colorFor(ev.space_id)};"
                  title={`${ev.title} · ${spaceLabel(ev.space_id)}`}
                >
                  {#if !ev.all_day}<span class="t">{timeLabel(ev)}</span>{/if}
                  <span class="ttl">{ev.title}</span>
                </span>
              {/each}
              {#if dayEvents.length > 3}
                <span class="more">+{dayEvents.length - 3}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/each}
    </main>

    <aside class="day-pane">
      <div class="day-pane-head">
        <div class="num">{selected.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'long' })}</div>
        <div class="label muted">
          {#if selectedKey === todayKey}heute · {/if}{selectedEvents.length} {selectedEvents.length === 1 ? 'termin' : 'termine'}
        </div>
      </div>
      <div class="day-list">
        {#each selectedEvents as ev (ev.id)}
          <a
            class="day-event"
            href={`/items/${ev.id}`}
            style="border-left-color: {colorFor(ev.space_id)};"
          >
            <span class="when muted">{fullTimeLabel(ev)} · <span class="space-tag">{spaceLabel(ev.space_id)}</span></span>
            <span class="title">{ev.title}</span>
          </a>
        {/each}
        {#if selectedEvents.length === 0}
          <div class="muted empty">keine termine an diesem tag</div>
        {/if}
      </div>
      <div class="add-row">
        <button type="button" class="add" disabled={busy} onclick={newEventOnSelected}>
          + neuer termin
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
    flex-shrink: 0;
  }
  .head .month { font-weight: 600; }
  .head .nav-btn {
    width: 28px; height: 28px;
    display: inline-flex; align-items: center; justify-content: center;
    padding: 0;
  }
  .head .small { font-size: 11px; }
  .legend {
    display: inline-flex; gap: 10px; align-items: center;
    font-size: 11px; color: var(--muted);
    margin-right: 12px;
    flex-wrap: wrap;
  }
  .legend-item { display: inline-flex; align-items: center; gap: 4px; }
  .swatch {
    display: inline-block;
    width: 8px; height: 8px;
  }
  .space-tag { color: var(--muted); }
  .layout {
    flex: 1; min-height: 0;
    display: grid;
    grid-template-columns: 1fr 280px;
    gap: 0;
  }
  .month-grid {
    border-right: 1px solid var(--border);
    display: grid;
    grid-template-rows: 24px repeat(6, 1fr);
    overflow: hidden;
    min-width: 0;
  }
  .dow-row {
    display: grid; grid-template-columns: repeat(7, 1fr);
    border-bottom: 1px solid var(--border);
    background: rgba(127, 127, 127, 0.04);
  }
  .dow-row .dow {
    text-align: center;
    border-right: 1px solid var(--border);
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 4px 0;
  }
  .dow-row .dow:last-child { border-right: none; }
  .week-row {
    display: grid; grid-template-columns: repeat(7, 1fr);
    border-bottom: 1px solid var(--border);
    min-height: 0;
  }
  .week-row:last-child { border-bottom: none; }
  .day-cell {
    border-right: 1px solid var(--border);
    padding: 4px 6px;
    overflow: hidden;
    cursor: pointer;
    display: flex; flex-direction: column;
    gap: 2px;
    min-height: 0;
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    text-align: left;
    border-top: none;
    border-bottom: none;
    border-left: none;
  }
  .day-cell:last-child { border-right: none; }
  .day-cell.other { color: var(--muted); }
  .day-cell.other .num { opacity: 0.4; }
  .day-cell:hover { background: rgba(127, 127, 127, 0.05); }
  .day-cell.today { background: rgba(31, 111, 235, 0.08); }
  :global([data-theme="dark"]) .day-cell.today,
  :global(:root:not([data-theme])) .day-cell.today { background: rgba(88, 166, 255, 0.10); }
  .day-cell.selected { box-shadow: inset 0 0 0 2px var(--accent); }
  .day-cell .num {
    font-weight: 600; font-size: 13px; align-self: flex-start;
  }
  .day-cell.today .num { color: var(--accent); }
  .pill {
    background: var(--accent);
    color: white;
    font-size: 10px;
    padding: 1px 4px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    display: flex; gap: 4px;
  }
  .pill .t { opacity: 0.85; }
  .pill .ttl { overflow: hidden; text-overflow: ellipsis; }
  .more { font-size: 10px; color: var(--muted); align-self: flex-end; }

  .day-pane {
    overflow-y: auto;
    display: flex; flex-direction: column;
  }
  .day-pane-head {
    border-bottom: 1px solid var(--border);
    padding: 10px 14px;
    flex-shrink: 0;
  }
  .day-pane-head .num { font-size: 18px; font-weight: 600; }
  .day-pane-head .label { font-size: 12px; margin-top: 2px; }
  .day-list { padding: 8px 14px; flex: 1; }
  .day-event {
    display: block;
    border-left: 3px solid var(--accent);
    background: rgba(127, 127, 127, 0.06);
    padding: 4px 8px;
    margin-bottom: 6px;
    color: var(--fg);
    text-decoration: none;
  }
  .day-event:hover { text-decoration: none; background: rgba(127, 127, 127, 0.12); }
  .day-event .when { display: block; font-size: 11px; }
  .day-event .title { font-weight: 500; }
  .empty {
    padding: 16px 0;
    text-align: center;
    font-size: 12px;
  }
  .add-row {
    border-top: 1px solid var(--border);
    padding: 8px 14px;
    flex-shrink: 0;
  }
  .add { width: 100%; }

  /* iPad portrait — collapse sidebar */
  @media (max-width: 820px) {
    .layout { grid-template-columns: 1fr; }
    .day-pane { display: none; }
  }
</style>
