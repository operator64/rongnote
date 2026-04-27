<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import {
    autocompletion,
    completionKeymap,
    type Completion,
    type CompletionContext,
    type CompletionResult
  } from '@codemirror/autocomplete';

  type Props = {
    value: string;
    onChange?: (next: string) => void;
    /// Provide a list of titles for [[wiki-link]] autocomplete. Re-evaluated
    /// on every completion request, so callers can return a dynamic source.
    wikiTitles?: () => string[];
  };

  let { value, onChange, wikiTitles }: Props = $props();

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let suppressNext = false;

  function makeWikiSource(provider: () => string[]) {
    return (context: CompletionContext): CompletionResult | null => {
      // Match `[[...` up to cursor. Stop at line breaks or stray brackets.
      const before = context.matchBefore(/\[\[[^\[\]\n]*$/);
      if (!before) return null;
      const query = before.text.slice(2).toLowerCase();
      const titles = provider();
      const seen = new Set<string>();
      const options: Completion[] = [];
      for (const t of titles) {
        const key = t.toLowerCase();
        if (seen.has(key)) continue;
        if (query && !key.includes(query)) continue;
        seen.add(key);
        options.push({
          label: t,
          apply: t + ']]',
          type: 'text'
        });
        if (options.length >= 30) break;
      }
      return {
        from: before.from + 2,
        to: before.to,
        options,
        validFor: /^[^\[\]\n]*$/
      };
    };
  }

  onMount(() => {
    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged) {
        const next = u.state.doc.toString();
        if (suppressNext) {
          suppressNext = false;
          return;
        }
        onChange?.(next);
      }
    });

    const extensions = [
      lineNumbers(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap]),
      markdown(),
      highlightActiveLine(),
      EditorView.theme({
        '&': {
          height: '100%',
          backgroundColor: 'var(--bg)',
          color: 'var(--fg)',
          fontFamily: 'var(--font-mono)',
          fontSize: '13px'
        },
        '.cm-content': {
          padding: '8px 0',
          caretColor: 'var(--fg)'
        },
        '.cm-cursor, .cm-dropCursor': {
          borderLeftColor: 'var(--fg)',
          borderLeftWidth: '1.5px'
        },
        '.cm-gutters': {
          backgroundColor: 'var(--bg)',
          color: 'var(--muted)',
          border: 'none',
          borderRight: '1px solid var(--border)'
        },
        '.cm-activeLine': { backgroundColor: 'rgba(127,127,127,0.06)' },
        '.cm-activeLineGutter': { backgroundColor: 'transparent' },
        '.cm-selectionBackground, ::selection': {
          backgroundColor: 'rgba(127,127,127,0.25)'
        },
        '.cm-tooltip-autocomplete': {
          backgroundColor: 'var(--bg)',
          border: '1px solid var(--border)',
          fontFamily: 'var(--font-mono)'
        },
        '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
          backgroundColor: 'rgba(127,127,127,0.18)',
          color: 'var(--fg)'
        },
        '&.cm-focused': { outline: 'none' }
      }),
      updateListener
    ];

    if (wikiTitles) {
      extensions.push(
        autocompletion({
          activateOnTyping: true,
          override: [makeWikiSource(wikiTitles)]
        })
      );
    }

    const state = EditorState.create({
      doc: value ?? '',
      extensions
    });
    view = new EditorView({ state, parent: host });
  });

  $effect(() => {
    if (!view) return;
    if (view.state.doc.toString() !== value) {
      suppressNext = true;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value ?? '' }
      });
    }
  });

  onDestroy(() => view?.destroy());
</script>

<div class="cm-host" bind:this={host}></div>

<style>
  .cm-host {
    height: 100%;
    overflow: auto;
  }
  .cm-host :global(.cm-editor) {
    height: 100%;
  }
</style>
