<script lang="ts">
  // Shared widget chrome: titled box with optional meta + action slot.
  type Props = {
    title: string;
    meta?: string;
    children: import('svelte').Snippet;
    actions?: import('svelte').Snippet;
  };
  let { title, meta = '', children, actions }: Props = $props();
</script>

<section class="widget">
  <div class="widget-head">
    <span class="title">{title}</span>
    {#if meta}<span class="meta">{meta}</span>{/if}
    {#if actions}<span class="action-slot">{@render actions()}</span>{/if}
  </div>
  <div class="widget-body">
    {@render children()}
  </div>
</section>

<style>
  .widget {
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }
  .widget-head {
    height: 28px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    background: rgba(127, 127, 127, 0.04);
  }
  .widget-head .title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .widget-head .meta {
    color: var(--muted);
    font-size: 11px;
    margin-left: auto;
  }
  .widget-head .action-slot {
    margin-left: auto;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .widget-head :global(button) {
    padding: 0 6px;
    height: 20px;
    font-size: 11px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--muted);
  }
  .widget-head :global(button:hover) {
    border-color: var(--border);
    color: var(--fg);
  }
  .widget-body {
    padding: 10px 12px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
</style>
