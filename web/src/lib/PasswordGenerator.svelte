<script lang="ts">
  import { DEFAULT_OPTIONS, generatePassword, type GenOptions } from '$lib/password';

  type Props = {
    onPick: (password: string) => void;
    onClose: () => void;
  };
  let { onPick, onClose }: Props = $props();

  let opts = $state<GenOptions>({ ...DEFAULT_OPTIONS });
  let candidate = $state(generatePassword(opts));

  function regen() {
    candidate = generatePassword(opts);
  }

  $effect(() => {
    // Re-roll whenever options change.
    void opts.length;
    void opts.lowercase;
    void opts.uppercase;
    void opts.digits;
    void opts.symbols;
    try {
      candidate = generatePassword(opts);
    } catch {
      candidate = '';
    }
  });
</script>

<div
  class="popover"
  role="dialog"
  aria-modal="true"
  aria-label="generate password"
  tabindex="-1"
  onkeydown={(e) => e.key === 'Escape' && onClose()}
>
  <div class="row">
    <code class="candidate grow">{candidate || '—'}</code>
    <button type="button" onclick={regen} title="re-roll">↻</button>
  </div>
  <div class="row">
    <label class="row">
      length
      <input type="range" min="8" max="64" bind:value={opts.length} />
      <span class="muted len">{opts.length}</span>
    </label>
  </div>
  <div class="charsets">
    <label><input type="checkbox" bind:checked={opts.lowercase} /> a-z</label>
    <label><input type="checkbox" bind:checked={opts.uppercase} /> A-Z</label>
    <label><input type="checkbox" bind:checked={opts.digits} /> 0-9</label>
    <label><input type="checkbox" bind:checked={opts.symbols} /> !@#</label>
  </div>
  <div class="row actions">
    <button type="button" onclick={onClose}>cancel</button>
    <span class="grow"></span>
    <button type="button" disabled={!candidate} onclick={() => candidate && onPick(candidate)}>
      use
    </button>
  </div>
</div>

<style>
  .popover {
    border: 1px solid var(--border);
    background: var(--bg);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 320px;
  }
  .candidate {
    font-family: var(--font-mono);
    font-size: 13px;
    overflow-x: auto;
    white-space: nowrap;
    padding: 4px 6px;
    border: 1px solid var(--border);
    user-select: all;
  }
  .charsets {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px 12px;
  }
  .charsets label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .charsets input {
    width: auto;
  }
  .len {
    width: 24px;
    text-align: right;
  }
  .actions {
    margin-top: 4px;
  }
</style>
