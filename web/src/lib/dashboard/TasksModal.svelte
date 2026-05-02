<script lang="ts">
  import { api, type ItemSummary } from '$lib/api';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import TaskCheckbox from '$lib/TaskCheckbox.svelte';

  /// Big-text view + manage modal for tasks. Mirrors the inline widget
  /// (open + done sections) but with bigger font, all rows visible, and
  /// per-row × delete affordance. Tap-to-toggle, inline title rename,
  /// + at the bottom to add a new task.

  type Props = { onClose: () => void };
  let { onClose }: Props = $props();

  let openTasks = $derived.by(() => {
    return items.list
      .filter((it) => it.type === 'task' && !it.done)
      .sort((a, b) => {
        const ad = a.due_at ?? '￿';
        const bd = b.due_at ?? '￿';
        if (ad !== bd) return ad < bd ? -1 : 1;
        return b.updated_at.localeCompare(a.updated_at);
      });
  });
  let doneTasks = $derived.by(() => {
    return items.list
      .filter((it) => it.type === 'task' && it.done)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  });

  function dueClass(due: string | null | undefined): string {
    if (!due) return '';
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const d = new Date(`${due}T00:00:00`);
    if (d < today) return 'due overdue';
    if (d.getTime() === today.getTime()) return 'due today';
    return 'due future';
  }
  function dueLabel(due: string | null | undefined): string {
    if (!due) return '';
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const d = new Date(`${due}T00:00:00`);
    const diff = Math.round((d.getTime() - today.getTime()) / 86_400_000);
    if (diff === 0) return 'heute';
    if (diff === 1) return 'morgen';
    if (diff === -1) return 'gestern';
    if (diff > 0 && diff < 7) return d.toLocaleDateString(undefined, { weekday: 'short' });
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }

  async function onToggle(e: MouseEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await items.toggleTaskDone(id);
    } catch (err) {
      console.error('toggle failed', err);
    }
  }

  // Inline title edit: enter the input → save on blur or Enter.
  let editingId = $state<string | null>(null);
  let editingTitle = $state('');
  let saving = $state(false);

  function startEdit(t: ItemSummary) {
    editingId = t.id;
    editingTitle = t.title;
  }
  async function commitEdit() {
    if (!editingId) return;
    const id = editingId;
    const newTitle = editingTitle.trim();
    editingId = null;
    const t = items.list.find((x) => x.id === id);
    if (!t || newTitle === t.title || !newTitle) return;
    saving = true;
    try {
      const updated = await api.updateItem(id, { title: newTitle });
      items.upsert(updated);
    } catch (err) {
      console.error('rename failed', err);
    } finally {
      saving = false;
    }
  }

  async function removeTask(id: string) {
    if (!confirm('task wirklich löschen?')) return;
    try {
      await api.deleteItem(id);
      items.remove(id);
    } catch (err) {
      console.error('delete failed', err);
      alert(err instanceof Error ? err.message : 'delete failed');
    }
  }

  // Inline create.
  let nTitle = $state('');
  let nDue = $state('');
  let creating = $state(false);
  let createError = $state('');
  async function create() {
    if (!nTitle.trim()) {
      createError = 'titel fehlt';
      return;
    }
    creating = true;
    createError = '';
    try {
      const item = await api.createItem({
        title: nTitle.trim(),
        type: 'task',
        space_id: spaces.active?.id,
        due_at: nDue || undefined
      });
      items.upsert(item);
      nTitle = '';
      nDue = '';
    } catch (err) {
      createError = err instanceof Error ? err.message : 'speichern fehlgeschlagen';
    } finally {
      creating = false;
    }
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
      <strong>tasks</strong>
      <span class="muted small">
        {openTasks.length} offen · {doneTasks.length} done
      </span>
      <span class="grow"></span>
      <button type="button" onclick={onClose} aria-label="close">×</button>
    </div>

    <div class="body">
      {#if openTasks.length === 0 && doneTasks.length === 0}
        <div class="muted empty">noch keine tasks. unten anlegen.</div>
      {:else}
        {#each openTasks as t (t.id)}
          <div class="task">
            <TaskCheckbox
              done={false}
              size={22}
              onToggle={(e) => onToggle(e, t.id)}
            />
            {#if editingId === t.id}
              <input
                class="title-input"
                bind:value={editingTitle}
                onblur={commitEdit}
                onkeydown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    commitEdit();
                  }
                  if (e.key === 'Escape') {
                    e.preventDefault();
                    editingId = null;
                  }
                }}
                disabled={saving}
              />
            {:else}
              <button
                type="button"
                class="title-btn"
                onclick={() => startEdit(t)}
                title="zum umbenennen tippen"
              >{t.title || '(ohne titel)'}</button>
            {/if}
            {#if t.due_at}
              <span class={dueClass(t.due_at)}>{dueLabel(t.due_at)}</span>
            {/if}
            <button
              type="button"
              class="rm"
              onclick={() => removeTask(t.id)}
              aria-label="löschen"
            >×</button>
          </div>
        {/each}

        {#if doneTasks.length > 0}
          {#if openTasks.length > 0}
            <div class="divider muted">erledigt</div>
          {/if}
          {#each doneTasks as t (t.id)}
            <div class="task done">
              <TaskCheckbox
                done={true}
                size={22}
                onToggle={(e) => onToggle(e, t.id)}
              />
              <span class="title-static">{t.title || '(ohne titel)'}</span>
              <button
                type="button"
                class="rm"
                onclick={() => removeTask(t.id)}
                aria-label="löschen"
              >×</button>
            </div>
          {/each}
        {/if}
      {/if}
    </div>

    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <input
        type="text"
        bind:value={nTitle}
        placeholder="neue task — titel"
        disabled={creating}
      />
      <input
        type="date"
        bind:value={nDue}
        disabled={creating}
        title="optional fällig"
      />
      <button type="submit" class="primary" disabled={creating || !nTitle.trim()}>
        {creating ? '…' : 'add'}
      </button>
      {#if createError}<span class="danger small">{createError}</span>{/if}
    </form>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 200;
    display: flex; justify-content: center; align-items: flex-start;
    padding: 5vh 12px;
  }
  .modal {
    width: min(680px, 96vw);
    max-height: 90vh;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex; flex-direction: column;
  }
  .head {
    display: flex; align-items: baseline; gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  .head strong { font-size: 22px; }
  .head .small { font-size: 12px; }
  .grow { flex: 1; }
  .head button {
    background: transparent; color: var(--fg);
    border: 1px solid var(--border);
    padding: 4px 12px;
    font: inherit;
    font-size: 16px;
    cursor: pointer;
  }
  .head button:hover { border-color: var(--fg); }

  .body {
    padding: 8px 18px;
    overflow-y: auto;
    flex: 1;
  }
  .task {
    display: flex; align-items: center; gap: 12px;
    padding: 10px 4px;
    border-bottom: 1px solid var(--border);
  }
  .task:last-child { border-bottom: none; }
  .title-btn {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    text-align: left;
    background: transparent; color: var(--fg);
    border: none;
    padding: 4px 0;
    font: inherit;
    font-size: 16px;
    cursor: text;
  }
  .title-btn:hover { color: var(--accent); }
  .title-static {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 16px;
    color: var(--muted);
    text-decoration: line-through;
  }
  .title-input {
    flex: 1; min-width: 0;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--accent);
    padding: 4px 6px;
    font: inherit;
    font-size: 16px;
  }
  .due {
    font-size: 13px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .due.future { color: var(--muted); }
  .due.today { color: var(--accent); font-weight: 600; }
  .due.overdue { color: var(--danger); }
  .rm {
    background: transparent;
    border: none;
    color: var(--muted);
    font-size: 18px;
    cursor: pointer;
    padding: 0 6px;
  }
  .rm:hover { color: var(--danger); }
  .divider {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 12px 0 6px;
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .empty { padding: 24px 8px; text-align: center; font-size: 14px; }

  .add {
    display: flex; align-items: center; gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .add input[type='text'] {
    flex: 1; min-width: 200px;
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 6px 10px;
    font: inherit;
    font-size: 15px;
  }
  .add input[type='date'] {
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 6px 8px;
    font: inherit;
    font-size: 14px;
  }
  .add button {
    background: var(--bg); color: var(--fg);
    border: 1px solid var(--border);
    padding: 6px 16px;
    cursor: pointer;
    font: inherit;
    font-size: 15px;
  }
  .add button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .add button:disabled { opacity: 0.5; cursor: default; }
  .small { font-size: 12px; }
</style>
