<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import UnlockPrompt from '$lib/UnlockPrompt.svelte';
  import { ensureReady } from '$lib/crypto';
  import { applyPrefs, prefs } from '$lib/prefs.svelte';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';

  let { children } = $props();

  // Routes that don't require auth at all.
  const PUBLIC_ROUTES = new Set(['/login', '/register', '/recovery']);
  // Subset that should NOT auto-bump logged-in users to /items — user might
  // be there intentionally (e.g. trying their recovery code while still
  // logged in, or following their own share link).
  const ALWAYS_ALLOW_ROUTES = new Set(['/recovery']);
  /// Anything under /share/<token> is a public share view. Auth-agnostic.
  function isPublicPrefix(path: string): boolean {
    return path.startsWith('/share/');
  }

  onMount(async () => {
    await Promise.all([ensureReady(), session.refresh()]);
    if (session.user) await vault.tryRestore();
    enforceAuth();
  });

  $effect(() => {
    if (!session.loading) enforceAuth();
  });

  // Push prefs to the DOM whenever they change. Initial paint is handled by
  // the inline script in app.html.
  $effect(() => {
    applyPrefs(prefs.theme, prefs.fontSize);
  });

  function enforceAuth() {
    const path = $page.url.pathname;
    const isPublic = PUBLIC_ROUTES.has(path) || isPublicPrefix(path);
    const alwaysAllow = ALWAYS_ALLOW_ROUTES.has(path) || isPublicPrefix(path);
    if (!session.user && !isPublic) {
      goto('/login', { replaceState: true });
    } else if (session.user && (path === '/' || isPublic) && !alwaysAllow) {
      goto('/items', { replaceState: true });
    }
  }

  let path = $derived($page.url.pathname);
  let isPublic = $derived(PUBLIC_ROUTES.has(path) || isPublicPrefix(path));
  // Don't show the unlock prompt on /recovery or /share/* — those flows
  // don't need a usable vault.
  let showUnlock = $derived(
    !!session.user && !vault.isUnlocked && !isPublic && path !== '/'
  );
</script>

{#if session.loading}
  <div class="muted" style="padding: 8px;">…</div>
{:else if showUnlock}
  <UnlockPrompt />
{:else}
  {@render children()}
{/if}
