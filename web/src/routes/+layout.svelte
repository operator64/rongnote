<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import UnlockPrompt from '$lib/UnlockPrompt.svelte';
  import { ensureReady } from '$lib/crypto';
  import { applyPrefs, prefs } from '$lib/prefs.svelte';
  import { session } from '$lib/session.svelte';
  import { spaces } from '$lib/spaces.svelte';
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
    if (session.user) {
      // tryRestore + spaces.refresh in parallel — spaces only needs auth,
      // not the unwrapped vault. Awaiting both before enforceAuth means
      // a kiosk-only user reloading the page lands on /dashboard with
      // no /items flicker.
      await Promise.all([vault.tryRestore(), spaces.refresh()]);
    }
    enforceAuth();
  });

  $effect(() => {
    if (!session.loading) enforceAuth();
  });

  /// Once spaces hydrate, kick a kiosk-only user off /items (which they
  /// arrived at as the safe default) onto /dashboard.
  $effect(() => {
    if (!session.user) return;
    if (!spaces.isKioskOnly) return;
    const path = $page.url.pathname;
    if (path.startsWith('/items') || path === '/') {
      goto('/dashboard', { replaceState: true });
    }
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
      // Kiosk-only users land on /dashboard — they have nothing to do
      // in /items. Defer the kiosk decision until spaces have loaded;
      // until then, drop them at /items as a safe default and the
      // dashboard will pick them up on the next enforceAuth tick once
      // spaces.list populates.
      const target = spaces.isKioskOnly ? '/dashboard' : '/items';
      goto(target, { replaceState: true });
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
