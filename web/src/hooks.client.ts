import type { HandleClientError } from '@sveltejs/kit';

/// SvelteKit's default error normaliser turns every uncaught exception
/// into "Internal Error" with no detail — useless when chasing iOS-only
/// crashes. handleError fires INSIDE the framework with the real error
/// object, so we stash it on window.__rongnote_lastError before SvelteKit
/// hides it. The +error.svelte page reads from there to surface the
/// actual message + stack to the user.
///
/// Returning here is what SvelteKit shows as `page.error.message`, so
/// returning a useful string also makes the page itself self-explanatory.
export const handleError: HandleClientError = ({ error, event, status, message }) => {
  const e = error as { message?: string; stack?: string; name?: string } | null;
  const real = e?.message ?? String(error ?? 'unknown');
  const stack = e?.stack;
  const name = e?.name;

  if (typeof window !== 'undefined') {
    (window as unknown as { __rongnote_lastError?: object }).__rongnote_lastError = {
      msg: real,
      stack,
      src: `${name ?? 'Error'} on ${event.url.pathname}`
    };
    // Also log to console so Safari devtools picks it up.
    // eslint-disable-next-line no-console
    console.error('[hooks.client] caught', { error, status, message, real, stack });
  }

  return {
    message: real
  };
};
