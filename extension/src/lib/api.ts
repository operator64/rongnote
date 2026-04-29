// Tiny HTTP client for the rongnote API, talking to whatever server the
// user configured in Options. Cookie-based session — same flow as the
// browser SPA, just from a WebExtension origin.

export interface PrecheckResponse {
  passphrase_salt: string;
  master_wrap_passphrase: string;
}

export interface UserView {
  id: string;
  email: string;
  passphrase_salt: string;
  master_wrap_passphrase: string;
  public_key: string;
  encrypted_private_key: string;
}

export interface ItemSummary {
  id: string;
  type: string;
  title: string;
  tags: string[];
  path: string;
  updated_at: string;
}

export interface Item {
  id: string;
  space_id: string;
  type: string;
  title: string;
  tags: string[];
  path: string;
  encrypted_body: string | null;
  wrapped_item_key: string | null;
  key_wrap?: 'master' | 'sealed' | null;
  updated_at: string;
}

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

export class Api {
  constructor(public base: string) {
    this.base = base.replace(/\/+$/, '');
  }

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const init: RequestInit = {
      method,
      credentials: 'include',
      headers: body !== undefined ? { 'content-type': 'application/json' } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined
    };
    const res = await fetch(`${this.base}${path}`, init);
    const text = await res.text();
    let data: unknown = null;
    if (text) {
      try {
        data = JSON.parse(text);
      } catch {
        throw new ApiError(res.status, 'bad_response', `non-JSON ${res.status}`);
      }
    }
    if (!res.ok) {
      const obj = (data ?? {}) as { error?: string; message?: string };
      throw new ApiError(res.status, obj.error ?? 'error', obj.message ?? res.statusText);
    }
    return data as T;
  }

  precheck(email: string) {
    return this.req<PrecheckResponse>('POST', '/api/v1/auth/precheck', { email });
  }

  login(email: string, auth_hash: string) {
    return this.req<UserView>('POST', '/api/v1/auth/login', { email, auth_hash });
  }

  me() {
    return this.req<UserView>('GET', '/api/v1/auth/me');
  }

  listItems(opts: { type?: string; space_id?: string } = {}) {
    const qs = new URLSearchParams();
    if (opts.type) qs.set('type', opts.type);
    if (opts.space_id) qs.set('space_id', opts.space_id);
    const q = qs.toString();
    return this.req<ItemSummary[]>('GET', `/api/v1/items${q ? `?${q}` : ''}`);
  }

  getItem(id: string) {
    return this.req<Item>('GET', `/api/v1/items/${id}`);
  }
}
