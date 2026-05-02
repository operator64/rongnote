<script lang="ts">
  /// Big digital clock for the dashboard. Lives next to the weather
  /// widget in the bottom-left cell. Ticks every second so the seconds
  /// readout stays accurate; the minute / hour display only changes
  /// when it needs to.

  let now = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(id);
  });

  let hh = $derived(now.getHours().toString().padStart(2, '0'));
  let mm = $derived(now.getMinutes().toString().padStart(2, '0'));
  let ss = $derived(now.getSeconds().toString().padStart(2, '0'));

  let dateLabel = $derived(
    now.toLocaleDateString(undefined, {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
      year: 'numeric'
    })
  );

  /// ISO 8601 week number (matches the calendar widget's KW label).
  let isoWeek = $derived.by(() => {
    const d = new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
    const dayNum = d.getUTCDay() || 7;
    d.setUTCDate(d.getUTCDate() + 4 - dayNum);
    const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
    return Math.ceil(((d.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7);
  });
</script>

<section class="clock">
  <div class="time">
    <span class="hm">{hh}:{mm}</span>
    <span class="sec">{ss}</span>
  </div>
  <div class="date">{dateLabel}</div>
  <div class="kw muted">KW {isoWeek}</div>
</section>

<style>
  .clock {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 12px;
    text-align: center;
  }
  .time {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .hm {
    font-size: clamp(40px, 9vw, 88px);
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .sec {
    font-size: clamp(16px, 2.4vw, 28px);
    color: var(--muted);
    font-weight: 400;
  }
  .date {
    margin-top: 8px;
    font-size: 13px;
    color: var(--fg);
  }
  .kw {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
</style>
