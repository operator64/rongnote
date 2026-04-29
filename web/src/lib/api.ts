// Thin API client. All paths are relative; in dev Vite proxies /api to :8080,
// in prod the same Rust server serves both the SPA and the API.

export type ItemType =
  | 'note'
  | 'secret'
  | 'file'
  | 'event'
  | 'task'
  | 'snippet'
  | 'bookmark'
  | 'list';

export interface UserView {
  id: string;
  email: string;
  passphrase_salt: string;
  master_wrap_passphrase: string;
  public_key: string;
  encrypted_private_key: string;
}

export interface PrecheckResponse {
  passphrase_salt: string;
  master_wrap_passphrase: string;
}

export interface RecoveryInitResponse {
  recovery_salt: string;
  master_wrap_recovery: string;
}

export interface ItemSummary {
  id: string;
  type: ItemType;
  title: string;
  tags: string[];
  path: string;
  updated_at: string;
  /// 'YYYY-MM-DD' for tasks, null/undefined otherwise.
  due_at?: string | null;
  done: boolean;
  pinned: boolean;
}

export interface Item {
  id: string;
  space_id: string;
  type: ItemType;
  title: string;
  tags: string[];
  path: string;
  encrypted_body: string | null;
  /// For personal-space items: the item_key wrapped under master_key
  /// (XSalsa20-Poly1305). For team-space items: the caller's sealed-box wrap
  /// of the item_key with their X25519 public key. The discriminator is
  /// `key_wrap`. Null when the item has no body yet.
  wrapped_item_key: string | null;
  /// 'master' for personal-space, 'sealed' for team-space, null when no body.
  key_wrap?: 'master' | 'sealed' | null;
  /// Hex-encoded sha256 for type='file', null otherwise.
  blob_sha256?: string | null;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
  due_at?: string | null;
  done: boolean;
  pinned: boolean;
}

/// Per-member sealed wrap of an item key, used in CreateItemInput and
/// UpdateItemInput for team-space items. `sealed_item_key` is base64.
export interface MemberKeyInput {
  user_id: string;
  sealed_item_key: string;
}

export interface ListItemsOptions {
  type?: ItemType;
  trash?: boolean;
  space_id?: string;
}

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const init: RequestInit = {
    method,
    credentials: 'include',
    headers: body !== undefined ? { 'content-type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined
  };
  const res = await fetch(path, init);
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: unknown = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      throw new ApiError(res.status, 'bad_response', `non-JSON ${res.status} response`);
    }
  }
  if (!res.ok) {
    const obj = (data ?? {}) as { error?: string; message?: string };
    throw new ApiError(res.status, obj.error ?? 'error', obj.message ?? res.statusText);
  }
  return data as T;
}

export interface RegisterPayload {
  email: string;
  passphrase_salt: string;
  recovery_salt: string;
  master_wrap_passphrase: string;
  master_wrap_recovery: string;
  auth_hash: string;
  public_key: string;
  encrypted_private_key: string;
}

export interface ResetPassphrasePayload {
  email: string;
  auth_hash: string;
  new_passphrase_salt: string;
  new_master_wrap_passphrase: string;
}

export interface CreateItemInput {
  type?: ItemType;
  title: string;
  encrypted_body?: string;
  /// Personal-space wrap. Mutually exclusive with member_keys.
  wrapped_item_key?: string;
  /// Team-space wraps — one entry per current member of the target space.
  member_keys?: MemberKeyInput[];
  blob_sha256?: string;
  tags?: string[];
  path?: string;
  due_at?: string | null;
  done?: boolean;
  space_id?: string;
}

export interface UploadBlobResponse {
  sha256: string;
  size: number;
  already_existed: boolean;
}

export interface PasskeyListItem {
  id: string;
  name: string;
  created_at: string;
  last_used_at?: string | null;
}

export interface ShareView {
  id: string;
  token: string;
  item_id: string;
  item_title: string;
  created_at: string;
  expires_at?: string | null;
  use_count: number;
}

export interface PublicShareView {
  item_type: ItemType;
  item_title: string;
  encrypted_payload: string;
  expires_at?: string | null;
}

export interface VersionSummary {
  id: string;
  version: number;
  title: string;
  created_at: string;
  created_by?: string | null;
}

export interface VersionDetail {
  id: string;
  version: number;
  title: string;
  encrypted_body: string | null;
  wrapped_item_key: string | null;
  created_at: string;
}

export interface Space {
  id: string;
  name: string;
  kind: 'personal' | 'team';
  owner_id: string;
  role: 'owner' | 'editor' | 'viewer';
  member_count: number;
  created_at: string;
}

export interface Member {
  user_id: string;
  email: string;
  role: 'owner' | 'editor' | 'viewer';
  /// base64 public key for sealing item keys (Phase B)
  public_key: string;
  joined_at: string;
}

export interface AuditEntry {
  id: string;
  user_id: string | null;
  space_id: string | null;
  item_id: string | null;
  item_title: string | null;
  item_type: ItemType | null;
  action: string;
  meta: Record<string, unknown> | null;
  ts: string;
}

export interface UpdateItemInput {
  title?: string;
  update_body?: boolean;
  encrypted_body?: string | null;
  wrapped_item_key?: string | null;
  /// Required when update_body=true on a team-space item — fresh wraps for
  /// the rotated item_key, one per current member.
  member_keys?: MemberKeyInput[];
  tags?: string[];
  path?: string;
  /// Set true to apply due_at (incl. clearing to null).
  update_due_at?: boolean;
  due_at?: string | null;
  done?: boolean;
  pinned?: boolean;
}

export const api = {
  register: (payload: RegisterPayload) =>
    request<UserView>('POST', '/api/v1/auth/register', payload),
  precheck: (email: string) =>
    request<PrecheckResponse>('POST', '/api/v1/auth/precheck', { email }),
  login: (email: string, auth_hash: string) =>
    request<UserView>('POST', '/api/v1/auth/login', { email, auth_hash }),
  logout: () => request<void>('POST', '/api/v1/auth/logout'),
  me: () => request<UserView>('GET', '/api/v1/auth/me'),
  recoveryInit: (email: string) =>
    request<RecoveryInitResponse>('POST', '/api/v1/auth/recovery_init', { email }),
  resetPassphrase: (payload: ResetPassphrasePayload) =>
    request<void>('POST', '/api/v1/auth/reset_passphrase', payload),

  passkeyRegisterBegin: () =>
    request<{ state_id: string; options: unknown }>(
      'POST',
      '/api/v1/auth/passkey/register/begin',
      {}
    ),
  passkeyRegisterFinish: (payload: {
    state_id: string;
    response: unknown;
    master_wrap_passkey: string;
    name?: string;
  }) =>
    request<{ id: string; name: string; created_at: string }>(
      'POST',
      '/api/v1/auth/passkey/register/finish',
      payload
    ),
  passkeyLoginBegin: () =>
    request<{ state_id: string; options: unknown }>(
      'POST',
      '/api/v1/auth/passkey/login/begin',
      {}
    ),
  passkeyLoginFinish: (payload: { state_id: string; response: unknown }) =>
    request<{ user: UserView; master_wrap_passkey: string }>(
      'POST',
      '/api/v1/auth/passkey/login/finish',
      payload
    ),
  listPasskeys: () =>
    request<PasskeyListItem[]>('GET', '/api/v1/auth/passkey'),
  deletePasskey: (id: string) =>
    request<void>('DELETE', `/api/v1/auth/passkey/${id}`),

  listItems: (opts: ListItemsOptions = {}) => {
    const params = new URLSearchParams();
    if (opts.type) params.set('type', opts.type);
    if (opts.trash) params.set('trash', 'true');
    if (opts.space_id) params.set('space_id', opts.space_id);
    const qs = params.toString();
    return request<ItemSummary[]>('GET', `/api/v1/items${qs ? `?${qs}` : ''}`);
  },
  createItem: (input: CreateItemInput) => request<Item>('POST', '/api/v1/items', input),
  getItem: (id: string) => request<Item>('GET', `/api/v1/items/${id}`),
  updateItem: (id: string, input: UpdateItemInput) =>
    request<Item>('PATCH', `/api/v1/items/${id}`, input),
  /// Soft delete by default; `hard: true` removes permanently.
  deleteItem: (id: string, opts: { hard?: boolean } = {}) =>
    request<void>('DELETE', `/api/v1/items/${id}${opts.hard ? '?hard=true' : ''}`),
  restoreItem: (id: string) => request<Item>('POST', `/api/v1/items/${id}/restore`),
  /// Move an item to another space. Caller wraps item_key for the target's
  /// kind: wrapped_item_key (master) for personal, member_keys (sealed) for team.
  moveItem: (
    id: string,
    payload: {
      target_space_id: string;
      wrapped_item_key?: string;
      member_keys?: MemberKeyInput[];
    }
  ) => request<Item>('POST', `/api/v1/items/${id}/move`, payload),

  uploadBlob: async (cipherBytes: Uint8Array): Promise<UploadBlobResponse> => {
    const form = new FormData();
    form.append('blob', new Blob([cipherBytes as BlobPart]));
    const res = await fetch('/api/v1/files', {
      method: 'POST',
      credentials: 'include',
      body: form
    });
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
    return data as UploadBlobResponse;
  },

  fileBlobUrl: (hexSha256: string) => `/api/v1/files/${hexSha256}`,

  listAuditLog: (limit = 100) =>
    request<AuditEntry[]>('GET', `/api/v1/audit_log?limit=${limit}`),

  // Axum 0.7 nested routes don't match trailing-slash — hit /spaces (no slash).
  listSpaces: () => request<Space[]>('GET', '/api/v1/spaces'),
  getSpace: (id: string) => request<Space>('GET', `/api/v1/spaces/${id}`),
  createSpace: (name: string) =>
    request<Space>('POST', '/api/v1/spaces', { name }),
  deleteSpace: (id: string) => request<void>('DELETE', `/api/v1/spaces/${id}`),
  listMembers: (spaceId: string) =>
    request<Member[]>('GET', `/api/v1/spaces/${spaceId}/members`),
  addMember: (
    spaceId: string,
    email: string,
    role: 'editor' | 'viewer',
    item_keys: { item_id: string; sealed_item_key: string }[] = []
  ) =>
    request<Member>('POST', `/api/v1/spaces/${spaceId}/members`, {
      email,
      role,
      item_keys
    }),
  setMemberRole: (spaceId: string, userId: string, role: 'editor' | 'viewer') =>
    request<void>('PATCH', `/api/v1/spaces/${spaceId}/members/${userId}`, { role }),
  removeMember: (spaceId: string, userId: string) =>
    request<void>('DELETE', `/api/v1/spaces/${spaceId}/members/${userId}`),
  lookupUser: (email: string) =>
    request<{ id: string; email: string; public_key: string }>(
      'POST',
      '/api/v1/spaces/lookup_user',
      { email }
    ),

  createShare: (
    itemId: string,
    payload: { encrypted_payload: string; expires_in_days?: number | null }
  ) => request<ShareView>('POST', `/api/v1/items/${itemId}/share`, payload),
  listShares: (itemId: string) =>
    request<ShareView[]>('GET', `/api/v1/items/${itemId}/shares`),
  revokeShare: (shareId: string) =>
    request<void>('DELETE', `/api/v1/shares/${shareId}`),
  publicShare: (token: string) =>
    request<PublicShareView>('GET', `/api/v1/share/${token}`),

  listVersions: (itemId: string) =>
    request<VersionSummary[]>('GET', `/api/v1/items/${itemId}/versions`),
  getVersion: (itemId: string, version: number) =>
    request<VersionDetail>('GET', `/api/v1/items/${itemId}/versions/${version}`),
  restoreVersion: (itemId: string, version: number) =>
    request<Item>('POST', `/api/v1/items/${itemId}/versions/${version}/restore`),

  fetchBlob: async (hexSha256: string): Promise<Uint8Array> => {
    const res = await fetch(`/api/v1/files/${hexSha256}`, { credentials: 'include' });
    if (!res.ok) {
      throw new ApiError(res.status, 'fetch_blob', res.statusText);
    }
    const buf = await res.arrayBuffer();
    return new Uint8Array(buf);
  }
};
