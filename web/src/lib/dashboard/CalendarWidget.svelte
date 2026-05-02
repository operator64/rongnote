<script lang="ts">
  import { goto } from '$app/navigation';
  import { items } from '$lib/items.svelte';
  import Widget from './Widget.svelte';

  /// Today + tomorrow events, derived from items.list (which already
  /// contains the union when the calendar route or any space switcher
  /// has hydrated it). Updates live as items.list mutates.

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
</script>

<Widget title="kalender" meta="heute · morgen">
  {#snippet actions()}
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
</style>
