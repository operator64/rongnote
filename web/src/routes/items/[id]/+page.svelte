<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import BookmarkEditor from '$lib/BookmarkEditor.svelte';
  import FileEditor from '$lib/FileEditor.svelte';
  import ListEditor from '$lib/ListEditor.svelte';
  import NoteEditor from '$lib/NoteEditor.svelte';
  import SecretEditor from '$lib/SecretEditor.svelte';
  import SnippetEditor from '$lib/SnippetEditor.svelte';
  import TaskEditor from '$lib/TaskEditor.svelte';
  import { api, ApiError, type Item } from '$lib/api';

  let id = $derived($page.params.id);
  let item = $state<Item | null>(null);
  let error = $state('');
  let loading = $state(true);

  $effect(() => {
    if (!id) return;
    let cancelled = false;
    loading = true;
    error = '';
    api
      .getItem(id)
      .then((it) => {
        if (cancelled) return;
        item = it;
      })
      .catch((err) => {
        if (cancelled) return;
        if (err instanceof ApiError && err.status === 404) {
          goto('/items', { replaceState: true });
          return;
        }
        error = err instanceof Error ? err.message : 'load failed';
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if loading}
  <div class="muted" style="padding: 8px;">…</div>
{:else if error || !item}
  <div class="danger" style="padding: 8px;">{error || 'not found'}</div>
{:else if item.type === 'secret'}
  <SecretEditor {item} />
{:else if item.type === 'file'}
  <FileEditor {item} />
{:else if item.type === 'task'}
  <TaskEditor {item} />
{:else if item.type === 'snippet'}
  <SnippetEditor {item} />
{:else if item.type === 'bookmark'}
  <BookmarkEditor {item} />
{:else if item.type === 'list'}
  <ListEditor {item} />
{:else}
  <NoteEditor {item} />
{/if}
