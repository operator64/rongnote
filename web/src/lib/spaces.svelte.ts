import { api, type Space } from './api';

class SpaceStore {
  list = $state<Space[]>([]);
  loading = $state(false);
  /// Active space id. null means "not yet loaded — defer to personal".
  activeId = $state<string | null>(null);

  active = $derived<Space | null>(
    this.list.find((s) => s.id === this.activeId) ?? this.personal()
  );

  /// True iff every team membership of this user is role='kiosk'. The
  /// personal space (always 'owner') is intentionally excluded — even
  /// kiosk display accounts get a personal space at register time, but
  /// it stays empty for them.
  ///
  /// Used to route post-login traffic straight to /dashboard and to
  /// hide the "back to items" / "lock vault" buttons there: a real
  /// kiosk on a wall display has no use for the items chrome.
  isKioskOnly = $derived<boolean>(
    this.list.length > 0 &&
      this.list.some((s) => s.kind === 'team' && s.role === 'kiosk') &&
      this.list.every(
        (s) => s.kind === 'personal' || s.role === 'kiosk'
      )
  );

  personal(): Space | null {
    return this.list.find((s) => s.kind === 'personal') ?? null;
  }

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      this.list = await api.listSpaces();
      if (!this.activeId || !this.list.some((s) => s.id === this.activeId)) {
        this.activeId = this.defaultActive()?.id ?? null;
      }
    } finally {
      this.loading = false;
    }
  }

  /// Default active-space picker, used at first login + after a space
  /// the user was on disappears.
  ///
  /// Kiosk-only users default to their first team membership — their
  /// personal space exists for crypto reasons (keypair lives there)
  /// but is always empty, so landing the dashboard on it would show
  /// nothing. For everyone else, personal is the right home base.
  private defaultActive(): Space | null {
    if (this.isKioskOnlyCalc()) {
      return this.list.find((s) => s.kind === 'team') ?? this.personal();
    }
    return this.personal();
  }

  /// Computed eagerly so refresh() can use it without waiting for the
  /// `isKioskOnly` $derived to settle. Same logic, plain function form.
  private isKioskOnlyCalc(): boolean {
    return (
      this.list.length > 0 &&
      this.list.some((s) => s.kind === 'team' && s.role === 'kiosk') &&
      this.list.every((s) => s.kind === 'personal' || s.role === 'kiosk')
    );
  }

  setActive(id: string): void {
    if (this.list.some((s) => s.id === id)) {
      this.activeId = id;
    }
  }

  upsert(space: Space): void {
    const idx = this.list.findIndex((s) => s.id === space.id);
    if (idx >= 0) {
      this.list = [...this.list.slice(0, idx), space, ...this.list.slice(idx + 1)];
    } else {
      this.list = [...this.list, space];
    }
  }

  remove(id: string): void {
    this.list = this.list.filter((s) => s.id !== id);
    if (this.activeId === id) {
      this.activeId = this.defaultActive()?.id ?? null;
    }
  }
}

export const spaces = new SpaceStore();
