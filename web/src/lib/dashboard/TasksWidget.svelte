<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import { items } from '$lib/items.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import TaskCheckbox from '$lib/TaskCheckbox.svelte';
  import Widget from './Widget.svelte';

  /// Tasks lane next to the shopping list. Open tasks first (sorted by
  /// due_at asc, no-due last), done tasks tucked at the bottom + capped.
  /// Inline + creates a new task without leaving the dashboard — kiosk
  /// users have no Cmd-K, so this is their only path.

  const SHOW_OPEN = 12;
  const SHOW_DONE = 4;

  let openTasks = $derived.by(() => {
    return items.list
      .filter((it) => it.type === 'task' && !it.done)
      .sort((a, b) => {
        const ad = a.due_at ?? '￿';
        const bd = b.due_at ?? '￿';
        if (ad !== bd) return ad < bd ? -1 : 1;
        return b.updated_at.localeCompare(a.updated_at);
      })
      .slice(0, SHOW_OPEN);
  });

  let doneTasks = $derived.by(() => {
    return items.list
      .filter((it) => it.type === 'task' && it.done)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .slice(0, SHOW_DONE);
  });

  let openCount = $derived(items.list.filter((it) => it.type === 'task' && !it.done).length);
  let doneCount = $derived(items.list.filter((it) => it.type === 'task' && it.done).length);

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

  // --- Inline create modal ---
  let modalOpen = $state(false);
  let nTitle = $state('');
  let nDue = $state('');
  let saving = $state(false);
  let saveError = $state('');

  function openCreate() {
    nTitle = '';
    nDue = '';
    saveError = '';
    modalOpen = true;
  }

  async function save() {
    if (!nTitle.trim()) {
      saveError = 'titel fehlt';
      return;
    }
    saving = true;
    saveError = '';
    try {
      const item = await api.createItem({
        title: nTitle.trim(),
        type: 'task',
        space_id: spaces.active?.id,
        due_at: nDue || undefined
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

<Widget
  title="tasks"
  meta={openCount + doneCount > 0 ? `${openCount} offen · ${doneCount} done` : ''}
>
  {#snippet actions()}
    <button type="button" onclick={openCreate} title="neue task">+</button>
  {/snippet}

  {#if openTasks.length === 0 && doneTasks.length === 0}
    <div class="muted empty">keine tasks. + drücken um eine anzulegen.</div>
  {:else}
    {#each openTasks as t (t.id)}
      <a class="task" href={`/items/${t.id}`}>
        <TaskCheckbox done={false} onToggle={(e) => onToggle(e, t.id)} />
        <span class="text" title={t.title}>{t.title || '(ohne titel)'}</span>
        {#if t.due_at}
          <span class={dueClass(t.due_at)}>{dueLabel(t.due_at)}</span>
        {/if}
      </a>
    {/each}
    {#if doneTasks.length > 0}
      {#if openTasks.length > 0}
        <div class="divider muted">erledigt</div>
      {/if}
      {#each doneTasks as t (t.id)}
        <a class="task done" href={`/items/${t.id}`}>
          <TaskCheckbox done={true} onToggle={(e) => onToggle(e, t.id)} />
          <span class="text" title={t.title}>{t.title || '(ohne titel)'}</span>
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
        <strong>neue task</strong>
        <span class="grow"></span>
        <button type="button" onclick={() => (modalOpen = false)}>schließen</button>
      </div>
      <div class="body">
        <label class="field">
          <span class="lbl">titel</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="text"
            bind:value={nTitle}
            placeholder="müll raus, geschirrspüler ausräumen, ..."
            autofocus
          />
        </label>
        <label class="field">
          <span class="lbl">fällig (optional)</span>
          <input type="date" bind:value={nDue} />
        </label>
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
  .task {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    color: var(--fg);
    text-decoration: none;
    min-width: 0;
  }
  .task:hover { text-decoration: none; background: rgba(127, 127, 127, 0.06); }
  .task:last-child { border-bottom: none; }
  .task .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .task.done .text {
    color: var(--muted);
    text-decoration: line-through;
  }
  .due {
    font-size: 11px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .due.future { color: var(--muted); }
  .due.today { color: var(--accent); font-weight: 600; }
  .due.overdue { color: var(--danger); }
  .divider {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 8px 0 4px;
    border-top: 1px solid var(--border);
    padding-top: 4px;
  }
  .empty {
    padding: 16px 4px;
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
