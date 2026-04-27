// Client-side WebAuthn / Passkey flow with PRF extension for vault unlock.
//
// Strategy:
//   - On register: server returns standard CreationChallengeResponse. We
//     inject the PRF extension before passing to navigator.credentials.
//     The authenticator returns a PRF output we use to derive a KEK that
//     wraps master_key. Server stores the wrap, sees no key material.
//   - On login: same dance with navigator.credentials.get().
//
// PRF salt is a single fixed value app-wide, so login doesn't have to
// know which user/credential to address before authentication completes.

import sodium from 'libsodium-wrappers-sumo';
import { ensureReady, fromBase64, open as openSeal, seal, toBase64 } from './crypto';
import { api, type UserView } from './api';

const APP_PRF_LABEL = 'rongnote-prf-v1';
const KEK_LABEL = 'rongnote-passkey-kek-v1';

let appPrfSaltCache: Uint8Array | null = null;
async function appPrfSalt(): Promise<Uint8Array> {
  if (!appPrfSaltCache) {
    const buf = await crypto.subtle.digest(
      'SHA-256',
      new TextEncoder().encode(APP_PRF_LABEL)
    );
    appPrfSaltCache = new Uint8Array(buf);
  }
  return appPrfSaltCache;
}

// --- Base64url <-> ArrayBuffer ---

function b64urlEncode(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
function b64urlDecode(s: string): Uint8Array {
  let t = s.replace(/-/g, '+').replace(/_/g, '/');
  while (t.length % 4) t += '=';
  const bin = atob(t);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function asArrayBuffer(input: string | BufferSource): ArrayBuffer {
  if (typeof input === 'string') return b64urlDecode(input).buffer as ArrayBuffer;
  if (input instanceof ArrayBuffer) return input;
  const view = input as ArrayBufferView;
  return view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength) as ArrayBuffer;
}

// --- Options decoding ---

interface RawCreationOptions {
  publicKey?: Record<string, unknown>;
  [k: string]: unknown;
}

function decodeCreationOptions(opts: RawCreationOptions): PublicKeyCredentialCreationOptions {
  const pub = (opts.publicKey ?? opts) as Record<string, unknown>;
  const decoded = { ...pub } as Record<string, unknown>;
  decoded.challenge = asArrayBuffer(pub.challenge as string);
  const user = pub.user as Record<string, unknown>;
  decoded.user = { ...user, id: asArrayBuffer(user.id as string) };
  if (Array.isArray(pub.excludeCredentials)) {
    decoded.excludeCredentials = (pub.excludeCredentials as Array<Record<string, unknown>>).map(
      (c) => ({ ...c, id: asArrayBuffer(c.id as string) })
    );
  }
  return decoded as unknown as PublicKeyCredentialCreationOptions;
}

function decodeRequestOptions(opts: RawCreationOptions): PublicKeyCredentialRequestOptions {
  const pub = (opts.publicKey ?? opts) as Record<string, unknown>;
  const decoded = { ...pub } as Record<string, unknown>;
  decoded.challenge = asArrayBuffer(pub.challenge as string);
  if (Array.isArray(pub.allowCredentials)) {
    decoded.allowCredentials = (pub.allowCredentials as Array<Record<string, unknown>>).map(
      (c) => ({ ...c, id: asArrayBuffer(c.id as string) })
    );
  }
  return decoded as unknown as PublicKeyCredentialRequestOptions;
}

// --- Response encoding ---

function encodeAttestation(cred: PublicKeyCredential) {
  const att = cred.response as AuthenticatorAttestationResponse;
  const transports =
    typeof (att as { getTransports?: () => string[] }).getTransports === 'function'
      ? (att as { getTransports: () => string[] }).getTransports()
      : [];
  return {
    id: cred.id,
    rawId: b64urlEncode(new Uint8Array(cred.rawId)),
    type: cred.type,
    response: {
      attestationObject: b64urlEncode(new Uint8Array(att.attestationObject)),
      clientDataJSON: b64urlEncode(new Uint8Array(att.clientDataJSON)),
      transports
    },
    extensions: (cred as PublicKeyCredential).getClientExtensionResults?.() ?? {},
    authenticatorAttachment:
      (cred as { authenticatorAttachment?: string }).authenticatorAttachment ?? null
  };
}

function encodeAssertion(cred: PublicKeyCredential) {
  const ass = cred.response as AuthenticatorAssertionResponse;
  return {
    id: cred.id,
    rawId: b64urlEncode(new Uint8Array(cred.rawId)),
    type: cred.type,
    response: {
      authenticatorData: b64urlEncode(new Uint8Array(ass.authenticatorData)),
      clientDataJSON: b64urlEncode(new Uint8Array(ass.clientDataJSON)),
      signature: b64urlEncode(new Uint8Array(ass.signature)),
      userHandle: ass.userHandle ? b64urlEncode(new Uint8Array(ass.userHandle)) : null
    },
    extensions: (cred as PublicKeyCredential).getClientExtensionResults?.() ?? {},
    authenticatorAttachment:
      (cred as { authenticatorAttachment?: string }).authenticatorAttachment ?? null
  };
}

// --- PRF helpers ---

interface PrfClientOutputs {
  prf?: { results?: { first?: ArrayBuffer | Uint8Array } };
}

function readPrf(cred: PublicKeyCredential): Uint8Array | null {
  const ext = cred.getClientExtensionResults() as unknown as PrfClientOutputs;
  const first = ext.prf?.results?.first;
  if (!first) return null;
  return first instanceof Uint8Array ? first : new Uint8Array(first);
}

async function deriveKek(prfOutput: Uint8Array): Promise<Uint8Array> {
  await ensureReady();
  return sodium.crypto_generichash(32, KEK_LABEL, prfOutput);
}

async function withPrf(
  opts: PublicKeyCredentialCreationOptions | PublicKeyCredentialRequestOptions
): Promise<typeof opts> {
  const salt = await appPrfSalt();
  const ext = ((opts as { extensions?: Record<string, unknown> }).extensions ?? {}) as Record<
    string,
    unknown
  >;
  return {
    ...opts,
    extensions: {
      ...ext,
      prf: { eval: { first: salt } }
    }
  } as typeof opts;
}

// --- Public API ---

export class PasskeyError extends Error {}

export function isPasskeySupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    !!window.PublicKeyCredential &&
    typeof navigator !== 'undefined' &&
    !!navigator.credentials
  );
}

/// Register a new passkey for the currently-signed-in user. Wraps the
/// in-memory master_key with a PRF-derived KEK and persists the wrapped key
/// on the server, alongside the WebAuthn credential.
export async function registerPasskey(input: {
  masterKey: Uint8Array;
  name?: string;
}): Promise<void> {
  if (!isPasskeySupported()) throw new PasskeyError('passkeys not supported');

  const begin = await api.passkeyRegisterBegin();
  const opts = (await withPrf(
    decodeCreationOptions(begin.options as RawCreationOptions)
  )) as PublicKeyCredentialCreationOptions;
  const cred = (await navigator.credentials.create({
    publicKey: opts
  })) as PublicKeyCredential | null;
  if (!cred) throw new PasskeyError('cancelled');

  const prf = readPrf(cred);
  if (!prf) {
    throw new PasskeyError(
      'authenticator does not support PRF — cannot bind vault to this passkey'
    );
  }
  const kek = await deriveKek(prf);
  const wrapped = seal(input.masterKey, kek);

  await api.passkeyRegisterFinish({
    state_id: begin.state_id,
    response: encodeAttestation(cred),
    master_wrap_passkey: toBase64(wrapped),
    name: input.name
  });
}

/// Sign in via passkey + unlock the vault in one step.
export async function loginWithPasskey(): Promise<{
  user: UserView;
  masterKey: Uint8Array;
}> {
  if (!isPasskeySupported()) throw new PasskeyError('passkeys not supported');

  const begin = await api.passkeyLoginBegin();
  const opts = (await withPrf(
    decodeRequestOptions(begin.options as RawCreationOptions)
  )) as PublicKeyCredentialRequestOptions;
  const cred = (await navigator.credentials.get({
    publicKey: opts,
    mediation: 'optional'
  })) as PublicKeyCredential | null;
  if (!cred) throw new PasskeyError('cancelled');

  const prf = readPrf(cred);
  if (!prf) throw new PasskeyError('PRF result missing — wrong authenticator?');

  const finish = await api.passkeyLoginFinish({
    state_id: begin.state_id,
    response: encodeAssertion(cred)
  });

  const kek = await deriveKek(prf);
  let masterKey: Uint8Array;
  try {
    masterKey = openSeal(fromBase64(finish.master_wrap_passkey), kek);
  } catch {
    throw new PasskeyError('vault unwrap failed — passkey may be from a different account');
  }
  return { user: finish.user, masterKey };
}
