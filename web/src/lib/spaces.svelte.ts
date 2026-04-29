import { api, type Space } from './api';

class SpaceStore {
  list = $state<Space[]>([]);
  loading = $state(false);
  /// Active space id. null means "not yet loaded — defer to personal".
  activeId = $state<string | null>(null);

  active = $derived<Space | null>(
    this.list.find((s) => s.id === this.activeId) ?? this.personal()
  );

  personal(): Space | null {
    return this.list.find((s) => s.kind === 'personal') ?? null;
  }

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      this.list = await api.listSpaces();
      // Pick the personal space as default if no active set yet.
      if (!this.activeId || !this.list.some((s) => s.id === this.activeId)) {
        const p = this.personal();
        this.activeId = p?.id ?? null;
      }
    } finally {
      this.loading = false;
    }
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
      this.activeId = this.personal()?.id ?? null;
    }
  }
}

export const spaces = new SpaceStore();
