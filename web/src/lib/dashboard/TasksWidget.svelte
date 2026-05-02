<script lang="ts">
  import { items } from '$lib/items.svelte';
  import TaskCheckbox from '$lib/TaskCheckbox.svelte';
  import TasksModal from './TasksModal.svelte';
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

  // Tap a task or the + button → open the bigger TasksModal which
  // handles toggle / rename / delete / add in one place.
  let modalOpen = $state(false);
</script>

<Widget
  title="tasks"
  meta={openCount + doneCount > 0 ? `${openCount} offen · ${doneCount} done` : ''}
>
  {#snippet actions()}
    <button type="button" onclick={() => (modalOpen = true)} title="neue task">+</button>
  {/snippet}

  {#if openTasks.length === 0 && doneTasks.length === 0}
    <div
      class="muted empty"
      role="button"
      tabindex="0"
      onclick={() => (modalOpen = true)}
      onkeydown={(ev) => ev.key === 'Enter' && (modalOpen = true)}
    >
      keine tasks. tippen um eine anzulegen.
    </div>
  {:else}
    {#each openTasks as t (t.id)}
      <div
        class="task"
        role="button"
        tabindex="0"
        onclick={() => (modalOpen = true)}
        onkeydown={(ev) => ev.key === 'Enter' && (modalOpen = true)}
      >
        <TaskCheckbox done={false} onToggle={(e) => onToggle(e, t.id)} />
        <span class="text" title={t.title}>{t.title || '(ohne titel)'}</span>
        {#if t.due_at}
          <span class={dueClass(t.due_at)}>{dueLabel(t.due_at)}</span>
        {/if}
      </div>
    {/each}
    {#if doneTasks.length > 0}
      {#if openTasks.length > 0}
        <div class="divider muted">erledigt</div>
      {/if}
      {#each doneTasks as t (t.id)}
        <div
          class="task done"
          role="button"
          tabindex="0"
          onclick={() => (modalOpen = true)}
          onkeydown={(ev) => ev.key === 'Enter' && (modalOpen = true)}
        >
          <TaskCheckbox done={true} onToggle={(e) => onToggle(e, t.id)} />
          <span class="text" title={t.title}>{t.title || '(ohne titel)'}</span>
        </div>
      {/each}
    {/if}
  {/if}
</Widget>

{#if modalOpen}
  <TasksModal onClose={() => (modalOpen = false)} />
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
    cursor: pointer;
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
    cursor: pointer;
  }
</style>
