// Per-device dashboard settings — which list to show, geo, transit stops.
// localStorage so each browser has its own dashboard. Could later sync via
// a special vault item if cross-device parity becomes useful.

const KEY = 'rongnote.dashboard';

export interface DashboardSettings {
  /// item id of the list to render in the lists widget. null = "first
  /// pinned list" (dropdown shows whatever pinned list comes first).
  list_id: string | null;
  /// Weather + transit center. null = ask geolocation on first load.
  geo: { lat: number; lon: number; place: string } | null;
  /// db-rest stop IDs (HAFAS / IBNR), max 2.
  stop_ids: string[];
  /// Fallback labels for the stop columns when the API lookup fails.
  stop_labels: string[];
}

function defaults(): DashboardSettings {
  return {
    list_id: null,
    geo: null,
    stop_ids: [],
    stop_labels: []
  };
}

class DashboardStore {
  s = $state<DashboardSettings>(defaults());
  loaded = $state(false);

  load(): void {
    if (typeof window === 'undefined') return;
    try {
      const raw = window.localStorage.getItem(KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as Partial<DashboardSettings>;
        const next = { ...defaults(), ...parsed };
        // v1.6 migration: db-rest IDs are 7-8 digit numerics (e.g.
        // 8000085 = Düsseldorf Hbf). VRR IDs are 8 digits starting
        // with 200 / 211 / etc. Anything starting with a leading 8
        // and 7-8 chars is almost certainly an old db-rest IBNR — drop
        // those so the user re-runs "find nearest" and gets fresh
        // VRR-format IDs.
        next.stop_ids = next.stop_ids.filter(
          (id) => !/^8\d{6,7}$/.test(id)
        );
        if (next.stop_ids.length !== next.stop_labels.length) {
          next.stop_labels = next.stop_labels.slice(0, next.stop_ids.length);
        }
        this.s = next;
      }
    } catch {
      // ignore corrupt JSON
    }
    this.loaded = true;
  }

  save(next: Partial<DashboardSettings>): void {
    this.s = { ...this.s, ...next };
    if (typeof window === 'undefined') return;
    try {
      window.localStorage.setItem(KEY, JSON.stringify(this.s));
    } catch {
      /* quota / privacy mode — silently ignored */
    }
  }
}

export const dashboardSettings = new DashboardStore();
