import { api, type UserView } from './api';

class Session {
  user = $state<UserView | null>(null);
  loading = $state(true);

  async refresh(): Promise<UserView | null> {
    try {
      this.user = await api.me();
    } catch {
      this.user = null;
    } finally {
      this.loading = false;
    }
    return this.user;
  }

  setUser(u: UserView | null): void {
    this.user = u;
    this.loading = false;
  }

  async logout(): Promise<void> {
    try {
      await api.logout();
    } finally {
      this.user = null;
    }
  }
}

export const session = new Session();
