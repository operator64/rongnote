import { api, type Item, type ItemSummary, type ItemType } from './api';

export interface ItemsFilter {
  type?: ItemType;
  tag?: string;
  pathPrefix?: string;
  q?: string;
}

export interface PathNode {
  path: string;
  name: string;
  depth: number;
  /// Items directly at this path.
  directCount: number;
  /// Items at this path or any descendant path.
  subtreeCount: number;
}

export type ViewMode = 'active' | 'trash';

class ItemStore {
  list = $state<ItemSummary[]>([]);
  loading = $state(false);
  filter = $state<ItemsFilter>({});
  view = $state<ViewMode>('active');

  filteredList = $derived.by(() => {
    const f = this.filter;
    const q = f.q?.trim().toLowerCase() ?? '';
    let result = this.list.filter((n) => {
      if (f.type && n.type !== f.type) return false;
      if (f.tag && !n.tags.includes(f.tag)) return false;
      if (f.pathPrefix && !pathMatches(n.path, f.pathPrefix)) return false;
      if (q && !n.title.toLowerCase().includes(q)) return false;
      return true;
    });
    if (f.type === 'task') {
      result = [...result].sort(taskCompare);
    }
    return result;
  });

  tagCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const n of this.list) {
      for (const t of n.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
    }
    return Array.from(counts, ([tag, count]) => ({ tag, count })).sort((a, b) =>
      a.tag.localeCompare(b.tag)
    );
  });

  /// Hierarchical path tree: every distinct path AND its ancestors get a
  /// node. Subtree counts roll up. Click "/projekte" → filter shows
  /// "/projekte" and everything under it via prefix match.
  pathTree = $derived.by<PathNode[]>(() => {
    const direct = new Map<string, number>();
    for (const n of this.list) direct.set(n.path, (direct.get(n.path) ?? 0) + 1);

    const subtree = new Map<string, number>();
    for (const [path, count] of direct) {
      const parts = path.split('/').filter(Boolean);
      let ancestor = '/';
      subtree.set(ancestor, (subtree.get(ancestor) ?? 0) + count);
      for (let i = 0; i < parts.length; i++) {
        ancestor = '/' + parts.slice(0, i + 1).join('/');
        subtree.set(ancestor, (subtree.get(ancestor) ?? 0) + count);
      }
    }

    const nodes: PathNode[] = [];
    for (const [path, subtreeCount] of subtree) {
      nodes.push({
        path,
        name: path === '/' ? '/' : path.slice(path.lastIndexOf('/') + 1),
        depth: path === '/' ? 0 : (path.match(/\//g)?.length ?? 0),
        directCount: direct.get(path) ?? 0,
        subtreeCount
      });
    }
    return nodes.sort((a, b) => a.path.localeCompare(b.path));
  });

  typeCounts = $derived.by(() => {
    const counts = new Map<ItemType, number>();
    for (const n of this.list) counts.set(n.type, (counts.get(n.type) ?? 0) + 1);
    // Always surface the user-creatable types so the "+" button has a way
    // to switch into them even when there are zero items yet.
    const ALWAYS_SHOW: ItemType[] = ['note', 'task', 'secret', 'file'];
    for (const t of ALWAYS_SHOW) if (!counts.has(t)) counts.set(t, 0);
    const order: ItemType[] = [
      'note',
      'secret',
      'snippet',
      'bookmark',
      'task',
      'event',
      'file'
    ];
    return order
      .filter((t) => counts.has(t))
      .map((type) => ({ type, count: counts.get(type)! }));
  });

  hasActiveFilter = $derived.by(() => {
    const f = this.filter;
    return !!(f.type || f.tag || (f.pathPrefix && f.pathPrefix !== '/') || f.q);
  });

  async refresh() {
    this.loading = true;
    try {
      this.list = await api.listItems({ trash: this.view === 'trash' });
    } finally {
      this.loading = false;
    }
  }

  async setView(view: ViewMode) {
    if (this.view === view) return;
    this.view = view;
    this.filter = {};
    await this.refresh();
  }

  upsert(item: Item) {
    const isTrashed = !!item.deleted_at;
    const matchesView = isTrashed === (this.view === 'trash');
    if (!matchesView) {
      // Item moved to the other view — drop from current list.
      this.remove(item.id);
      return;
    }
    const summary: ItemSummary = {
      id: item.id,
      type: item.type,
      title: item.title,
      tags: item.tags,
      path: item.path,
      updated_at: item.updated_at,
      due_at: item.due_at ?? null,
      done: item.done
    };
    const idx = this.list.findIndex((n) => n.id === item.id);
    if (idx >= 0) {
      this.list = [summary, ...this.list.slice(0, idx), ...this.list.slice(idx + 1)];
    } else {
      this.list = [summary, ...this.list];
    }
  }

  remove(id: string) {
    this.list = this.list.filter((n) => n.id !== id);
  }

  setFilter(next: ItemsFilter) {
    this.filter = next;
  }

  clearFilter() {
    this.filter = {};
  }

  /// Optimistic toggle of a task's done state. Reverts on server error.
  async toggleTaskDone(id: string): Promise<void> {
    const idx = this.list.findIndex((n) => n.id === id);
    if (idx < 0) return;
    const before = this.list[idx];
    if (before.type !== 'task') return;
    const after: ItemSummary = { ...before, done: !before.done };
    this.list = [...this.list.slice(0, idx), after, ...this.list.slice(idx + 1)];
    try {
      const updated = await api.updateItem(id, { done: after.done });
      this.upsert(updated);
    } catch (err) {
      // Revert
      const revertIdx = this.list.findIndex((n) => n.id === id);
      if (revertIdx >= 0) {
        this.list = [
          ...this.list.slice(0, revertIdx),
          before,
          ...this.list.slice(revertIdx + 1)
        ];
      }
      throw err;
    }
  }
}

/// open tasks first by due_at asc (with null at the end of the open group),
/// done tasks at the bottom.
function taskCompare(a: ItemSummary, b: ItemSummary): number {
  if (a.done !== b.done) return a.done ? 1 : -1;
  const ad = a.due_at ?? null;
  const bd = b.due_at ?? null;
  if (ad && !bd) return -1;
  if (!ad && bd) return 1;
  if (ad && bd && ad !== bd) return ad.localeCompare(bd);
  return b.updated_at.localeCompare(a.updated_at);
}

function pathMatches(path: string, prefix: string): boolean {
  if (prefix === '/') return true;
  if (path === prefix) return true;
  return path.startsWith(prefix.endsWith('/') ? prefix : prefix + '/');
}

export function tagAncestors(tag: string): string[] {
  const parts = tag.split('/');
  const out: string[] = [];
  for (let i = 1; i <= parts.length; i++) {
    out.push(parts.slice(0, i).join('/'));
  }
  return out;
}

export function parseTagInput(input: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of input.split(',')) {
    const t = raw.trim().replace(/^#+/, '').toLowerCase();
    if (!t) continue;
    if (seen.has(t)) continue;
    seen.add(t);
    out.push(t);
  }
  return out;
}

export function formatTagInput(tags: string[]): string {
  return tags.map((t) => `#${t}`).join(', ');
}

export function normalizePath(input: string): string {
  let p = input.trim();
  if (!p) return '/';
  if (!p.startsWith('/')) p = '/' + p;
  if (p.length > 1 && p.endsWith('/')) p = p.slice(0, -1);
  return p;
}

export const items = new ItemStore();
