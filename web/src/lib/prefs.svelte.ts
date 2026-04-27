/// User preferences persisted across sessions.
///
/// theme:
///   - 'light' | 'dark' force a theme regardless of system setting
///   - 'auto' follows prefers-color-scheme
/// fontSize:
///   - base text size in px (other sizes inherit or stay fixed)
///
/// Stored in localStorage (not sessionStorage) so prefs outlive logout.
/// Initial application happens via an inline script in app.html to avoid
/// FOUC; this class keeps it in sync after that.

export type Theme = 'light' | 'dark' | 'auto';

const STORAGE_KEY = 'rongnote.prefs';
const DEFAULT_FONT_SIZE = 13;
const MIN_FONT_SIZE = 11;
const MAX_FONT_SIZE = 20;
const FONT_STEP = 1;

interface Persisted {
  theme: Theme;
  fontSize: number;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

class Prefs {
  theme = $state<Theme>('auto');
  fontSize = $state(DEFAULT_FONT_SIZE);

  constructor() {
    if (typeof window === 'undefined') return;
    try {
      const raw = window.localStorage.getItem(STORAGE_KEY);
      if (!raw) return;
      const p = JSON.parse(raw) as Partial<Persisted>;
      if (p.theme === 'light' || p.theme === 'dark' || p.theme === 'auto') {
        this.theme = p.theme;
      }
      if (typeof p.fontSize === 'number' && Number.isFinite(p.fontSize)) {
        this.fontSize = clamp(p.fontSize, MIN_FONT_SIZE, MAX_FONT_SIZE);
      }
    } catch {
      /* ignore — fresh defaults */
    }
  }

  setTheme(t: Theme): void {
    this.theme = t;
    this.persist();
  }

  /// Cycle auto → light → dark → auto …
  cycleTheme(): void {
    const order: Theme[] = ['auto', 'light', 'dark'];
    const i = order.indexOf(this.theme);
    this.setTheme(order[(i + 1) % order.length]);
  }

  bumpFontSize(delta: number): void {
    this.fontSize = clamp(this.fontSize + delta * FONT_STEP, MIN_FONT_SIZE, MAX_FONT_SIZE);
    this.persist();
  }

  resetFontSize(): void {
    this.fontSize = DEFAULT_FONT_SIZE;
    this.persist();
  }

  private persist(): void {
    if (typeof window === 'undefined') return;
    const data: Persisted = { theme: this.theme, fontSize: this.fontSize };
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  }
}

export const prefs = new Prefs();

/// Imperatively apply prefs to the DOM. Called from a top-level $effect so
/// changes take effect on toggle. The inline script in app.html handles the
/// initial paint to avoid a flash.
export function applyPrefs(theme: Theme, fontSize: number): void {
  if (typeof document === 'undefined') return;
  if (theme === 'auto') {
    document.documentElement.removeAttribute('data-theme');
  } else {
    document.documentElement.setAttribute('data-theme', theme);
  }
  document.documentElement.style.setProperty('--base-font-size', `${fontSize}px`);
}
