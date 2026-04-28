<script lang="ts">
  import {
    Bookmark,
    Calendar,
    Code,
    File,
    FileText,
    Key,
    ListChecks,
    ListTodo
  } from '@lucide/svelte';
  import type { ItemType } from '$lib/api';

  type Props = { type: ItemType; size?: number; class?: string };
  let { type, size = 14, class: className = '' }: Props = $props();

  const map = {
    note: FileText,
    task: ListTodo,
    secret: Key,
    file: File,
    snippet: Code,
    bookmark: Bookmark,
    event: Calendar,
    list: ListChecks
  } as const;

  let Component = $derived(map[type] ?? FileText);
</script>

<Component {size} class="item-icon {className}" strokeWidth={1.5} />

<style>
  :global(.item-icon) {
    flex-shrink: 0;
    color: var(--muted);
    vertical-align: -3px;
  }
</style>
