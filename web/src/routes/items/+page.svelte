<script lang="ts">
  import { items } from '$lib/items.svelte';
</script>

<div class="empty">
  <div>
    <p class="muted">select an item from the sidebar, or create one with +.</p>
    {#if items.list.length === 0}
      <p class="muted">your personal space is empty.</p>
    {/if}
    <p class="muted hint">press Ctrl-K for command palette.</p>
  </div>
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    text-align: center;
  }
  .hint {
    font-size: 11px;
    margin-top: 24px;
  }
</style>
