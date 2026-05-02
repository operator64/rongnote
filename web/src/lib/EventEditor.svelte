<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, type Item } from '$lib/api';
  import { decryptItemBody, encryptBodyForSpace } from '$lib/itemCrypto';
  import {
    formatTagInput,
    items,
    normalizePath,
    parseTagInput
  } from '$lib/items.svelte';
  import { vault } from '$lib/vault.svelte';

  type EventPayload = {
    location: string;
    description: string;
  };

  function emptyPayload(): EventPayload {
    return { location: '', description: '' };
  }

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let allDay = $state(!!initial.all_day);
  // datetime-local <input> values: 'YYYY-MM-DDTHH:mm' in local timezone
  // svelte-ignore state_referenced_locally
  let startLocal = $state(toLocalInput(initial.start_at, !!initial.all_day, false));
  // svelte-ignore state_referenced_locally
  let endLocal = $state(toLocalInput(initial.end_at, !!initial.all_day, true));
  // svelte-ignore state_referenced_locally
  let payload = $state<EventPayload>(decryptPayload(initial));
  // svelte-ignore state_referenced_locally
  let tagsInput = $state(formatTagInput(initial.tags));
  // svelte-ignore state_referenced_locally
  let pathInput = $state(initial.path);
  // svelte-ignore state_referenced_locally
  let lastSavedAt = $state<Date | null>(new Date(initial.updated_at));
  let error = $state('');
  let saving = $state(false);
  let dirty = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  let trashed = $derived(!!item.deleted_at);

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      allDay = !!initial.all_day;
      startLocal = toLocalInput(initial.start_at, !!initial.all_day, false);
      endLocal = toLocalInput(initial.end_at, !!initial.all_day, true);
      payload = decryptPayload(initial);
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      error = '';
      lastSavedAt = new Date(initial.updated_at);
    }
  });

  /// Convert a UTC RFC3339 string to a `<input type="datetime-local|date">`
  /// value in the user's local timezone. For all-day events the saved
  /// `end_at` is exclusive (next-midnight), so for the date input we
  /// subtract one day.
  function toLocalInput(
    iso: string | null | undefined,
    asDate: boolean,
    isEnd: boolean
  ): string {
    if (!iso) return '';
    const d = new Date(iso);
    if (asDate) {
      // For all-day end, server stores exclusive midnight of the day after.
      // Show the *inclusive* last day in the input.
      const display = new Date(d);
      if (isEnd) display.setDate(display.getDate() - 1);
      return ymd(display);
    }
    // datetime-local needs YYYY-MM-DDTHH:mm in local time
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  function ymd(d: Date): string {
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  /// Convert a local `<input>` value to a UTC RFC3339 string for the API.
  /// For all-day, end is the start of the *next* day after the displayed
  /// last day (iCal DTEND convention).
  function fromLocalInput(
    s: string,
    asDate: boolean,
    isEnd: boolean
  ): string | null {
    if (!s) return null;
    if (asDate) {
      // Treat the date as midnight local. For end, push to next day.
      const [y, m, day] = s.split('-').map(Number);
      const local = new Date(y, m - 1, day, 0, 0, 0);
      if (isEnd) local.setDate(local.getDate() + 1);
      return local.toISOString();
    }
    // datetime-local: 'YYYY-MM-DDTHH:mm' parses as local
    const local = new Date(s);
    if (Number.isNaN(local.getTime())) return null;
    return local.toISOString();
  }

  function decryptPayload(it: Item): EventPayload {
    if (!it.encrypted_body || !it.wrapped_item_key) return emptyPayload();
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) return emptyPayload();
    try {
      const text = decryptItemBody(it, vault.masterKey, vault.publicKey, vault.privateKey);
      const parsed = JSON.parse(text) as Partial<EventPayload>;
      return { ...emptyPayload(), ...parsed };
    } catch {
      return emptyPayload();
    }
  }

  function scheduleSave() {
    if (trashed) return;
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, SAVE_DEBOUNCE_MS);
  }

  function onAllDayToggle() {
    // When toggling all-day, re-interpret the displayed values to match
    // the new mode so the user doesn't have to retype.
    if (allDay) {
      // switched ON — keep the date portion of whatever was there
      startLocal = startLocal.split('T')[0] || ymd(new Date());
      endLocal = endLocal.split('T')[0] || startLocal;
    } else {
      // switched OFF — append a default time
      if (startLocal && !startLocal.includes('T')) startLocal = `${startLocal}T09:00`;
      if (endLocal && !endLocal.includes('T')) endLocal = `${endLocal}T10:00`;
    }
    scheduleSave();
  }

  async function saveNow() {
    if (saving || !dirty) return;
    if (!vault.masterKey) {
      error = 'vault locked — cannot save';
      return;
    }
    saving = true;
    const titleSnap = title || 'Untitled event';
    const startSnap = fromLocalInput(startLocal, allDay, false);
    const endSnap = fromLocalInput(endLocal, allDay, true);
    const allDaySnap = allDay;
    const payloadSnap = JSON.stringify(payload);
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      if (!vault.publicKey || !vault.privateKey) throw new Error('vault locked');
      const wrap = await encryptBodyForSpace({
        body: payloadSnap,
        spaceId: item.space_id,
        masterKey: vault.masterKey,
        publicKey: vault.publicKey,
        privateKey: vault.privateKey,
        item
      });
      const updated = await api.updateItem(item.id, {
        title: titleSnap,
        tags: tagsSnap,
        path: pathSnap,
        update_body: true,
        ...wrap,
        update_event_time: true,
        start_at: startSnap,
        end_at: endSnap,
        all_day: allDaySnap
      });
      item = updated;
      lastSavedAt = new Date(updated.updated_at);
      items.upsert(updated);
    } catch (err) {
      dirty = true;
      error = err instanceof Error ? err.message : 'save failed';
    } finally {
      saving = false;
      if (dirty) {
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(saveNow, SAVE_DEBOUNCE_MS);
      }
    }
  }

  async function del() {
    if (!confirm('move this event to trash?')) return;
    try {
      await api.deleteItem(item.id);
      items.remove(item.id);
      goto('/items', { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'delete failed';
    }
  }
  async function restoreItem() {
    try {
      const restored = await api.restoreItem(item.id);
      items.remove(item.id);
      goto(`/items/${restored.id}`, { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'restore failed';
    }
  }
  async function hardDelete() {
    if (!confirm('permanently delete this event?')) return;
    try {
      await api.deleteItem(item.id, { hard: true });
      items.remove(item.id);
      goto('/items', { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'delete failed';
    }
  }

  let savedLabel = $derived.by(() => {
    if (saving) return 'saving…';
    if (dirty) return 'unsaved';
    if (!lastSavedAt) return '';
    const diff = (Date.now() - lastSavedAt.getTime()) / 1000;
    if (diff < 5) return 'saved';
    if (diff < 60) return `saved ${Math.floor(diff)}s ago`;
    return `saved ${lastSavedAt.toLocaleTimeString()}`;
  });
</script>

{#if trashed}
  <div class="trash-banner row">
    <span class="grow">in trash · changes disabled</span>
    <button type="button" onclick={restoreItem}>restore</button>
    <button type="button" class="danger" onclick={hardDelete}>delete forever</button>
  </div>
{/if}

<div class="head row" class:dimmed={trashed}>
  <input
    class="title-input grow"
    type="text"
    placeholder="event title"
    bind:value={title}
    oninput={scheduleSave}
    readonly={trashed}
  />
  {#if !trashed}
    <button class="danger" onclick={del}>delete</button>
  {/if}
</div>

<div class="time-row" class:dimmed={trashed}>
  <label class="all-day">
    <input
      type="checkbox"
      bind:checked={allDay}
      onchange={onAllDayToggle}
      disabled={trashed}
    />
    <span>ganztägig</span>
  </label>
  <span class="grow"></span>
</div>

<div class="time-row" class:dimmed={trashed}>
  <label class="field">
    <span class="label">start</span>
    {#if allDay}
      <input type="date" bind:value={startLocal} oninput={scheduleSave} readonly={trashed} />
    {:else}
      <input type="datetime-local" bind:value={startLocal} oninput={scheduleSave} readonly={trashed} />
    {/if}
  </label>
  <label class="field">
    <span class="label">ende</span>
    {#if allDay}
      <input type="date" bind:value={endLocal} oninput={scheduleSave} readonly={trashed} />
    {:else}
      <input type="datetime-local" bind:value={endLocal} oninput={scheduleSave} readonly={trashed} />
    {/if}
  </label>
</div>

<div class="meta-row row" class:dimmed={trashed}>
  <input
    class="meta-input"
    type="text"
    placeholder="ort"
    bind:value={payload.location}
    oninput={scheduleSave}
    readonly={trashed}
  />
  <input
    class="meta-input"
    type="text"
    placeholder="tags"
    bind:value={tagsInput}
    oninput={scheduleSave}
    readonly={trashed}
  />
  <input
    class="meta-input path"
    type="text"
    placeholder="/path"
    bind:value={pathInput}
    oninput={scheduleSave}
    readonly={trashed}
  />
</div>

<div class="body">
  <textarea
    placeholder="beschreibung, notizen, links…"
    bind:value={payload.description}
    oninput={scheduleSave}
    readonly={trashed}
  ></textarea>
</div>

<div class="meta row">
  {#if error}
    <span class="danger">{error}</span>
  {:else}
    <span class="muted">{savedLabel}</span>
  {/if}
  <span class="grow"></span>
  <span class="muted" title="encrypted on this device">e2e · {item.id.slice(0, 8)}</span>
</div>

<style>
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
  }
  .title-input {
    border: none;
    background: transparent;
    font-weight: 600;
    padding: 0 4px;
  }
  .title-input:focus { outline: none; }
  .time-row {
    display: flex; align-items: center; gap: 12px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .all-day {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 12px; color: var(--muted);
  }
  .all-day input { width: auto; margin: 0; }
  .field {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 12px;
  }
  .field .label { color: var(--muted); }
  .field input { padding: 2px 6px; }
  .meta-row {
    height: 26px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
  }
  .meta-input {
    flex: 1 1 auto;
    border: none;
    background: transparent;
    font-size: 12px;
    color: var(--muted);
    padding: 0 4px;
    min-width: 0;
  }
  .meta-input:focus { outline: none; color: var(--fg); }
  .meta-input.path { flex: 0 1 200px; }
  .body {
    flex: 1;
    overflow: hidden;
    display: flex;
  }
  .body textarea {
    flex: 1;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 12px 16px;
    resize: none;
    outline: none;
  }
  .meta {
    height: 22px;
    padding: 0 8px;
    border-top: 1px solid var(--border);
    font-size: 12px;
  }
  .trash-banner {
    background: rgba(127, 127, 127, 0.1);
    border-bottom: 1px solid var(--border);
    padding: 4px 8px;
    gap: 8px;
    font-size: 12px;
  }
  .dimmed { opacity: 0.7; }
</style>
