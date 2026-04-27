<script lang="ts">
  import { Square, SquareCheckBig } from '@lucide/svelte';

  type Props = {
    done: boolean;
    size?: number;
    disabled?: boolean;
    onToggle: (e: MouseEvent) => void;
    label?: string;
  };
  let {
    done,
    size = 16,
    disabled = false,
    onToggle,
    label = ''
  }: Props = $props();
</script>

<button
  type="button"
  class="task-checkbox"
  class:done
  {disabled}
  onclick={onToggle}
  aria-label={label || (done ? 'mark not done' : 'mark done')}
  aria-pressed={done}
>
  {#if done}
    <SquareCheckBig {size} strokeWidth={1.75} />
  {:else}
    <Square {size} strokeWidth={1.5} />
  {/if}
</button>

<style>
  .task-checkbox {
    border: none;
    background: transparent;
    padding: 0;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    flex-shrink: 0;
  }
  .task-checkbox:hover:not(:disabled) {
    color: var(--fg);
  }
  .task-checkbox.done {
    color: var(--accent);
  }
  .task-checkbox:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .task-checkbox:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }
</style>
