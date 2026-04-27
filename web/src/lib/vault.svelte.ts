import {
  deriveKekFromPassphrase,
  ensureReady,
  fromBase64,
  open,
  toBase64
} from './crypto';

const STORAGE_KEY = 'rongnote.vault';
const IDLE_LOCK_MS = 15 * 60 * 1000; // 15 minutes
const IDLE_EVENTS = ['mousemove', 'keydown', 'click', 'touchstart', 'scroll'] as const;

interface Persisted {
  master_key: string;
  public_key: string;
  private_key: string;
}

class Vault {
  masterKey = $state<Uint8Array | null>(null);
  publicKey = $state<Uint8Array | null>(null);
  privateKey = $state<Uint8Array | null>(null);

  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private idleHandler: (() => void) | null = null;

  get isUnlocked(): boolean {
    return this.masterKey !== null;
  }

  /// Try to restore the vault from sessionStorage. Returns true if restored.
  async tryRestore(): Promise<boolean> {
    if (typeof window === 'undefined') return false;
    const raw = window.sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return false;
    try {
      await ensureReady();
      const p = JSON.parse(raw) as Persisted;
      this.masterKey = fromBase64(p.master_key);
      this.publicKey = fromBase64(p.public_key);
      this.privateKey = fromBase64(p.private_key);
      this.startIdleWatch();
      return true;
    } catch {
      window.sessionStorage.removeItem(STORAGE_KEY);
      return false;
    }
  }

  /// Used after register or login: master_key is already in hand, just
  /// install it and unwrap the private key.
  async installAndUnwrap(input: {
    masterKey: Uint8Array;
    publicKey: Uint8Array;
    encryptedPrivateKey: Uint8Array;
  }): Promise<void> {
    await ensureReady();
    let sk: Uint8Array;
    try {
      sk = open(input.encryptedPrivateKey, input.masterKey);
    } catch {
      throw new Error('vault decryption failed (wrong master key?)');
    }
    this.masterKey = input.masterKey;
    this.publicKey = input.publicKey;
    this.privateKey = sk;
    this.persist();
  }

  /// Used after register: privateKey was just generated locally, no need to
  /// unwrap. Saves a roundtrip through secretbox.
  installFresh(input: {
    masterKey: Uint8Array;
    publicKey: Uint8Array;
    privateKey: Uint8Array;
  }): void {
    this.masterKey = input.masterKey;
    this.publicKey = input.publicKey;
    this.privateKey = input.privateKey;
    this.persist();
  }

  /// Used by the unlock prompt: only the passphrase + the wrapped values from
  /// the existing session are at hand.
  async unlockFromPassphrase(input: {
    passphrase: string;
    passphraseSalt: Uint8Array;
    masterWrapPassphrase: Uint8Array;
    publicKey: Uint8Array;
    encryptedPrivateKey: Uint8Array;
  }): Promise<void> {
    await ensureReady();
    const kek = await deriveKekFromPassphrase(input.passphrase, input.passphraseSalt);
    let mk: Uint8Array;
    try {
      mk = open(input.masterWrapPassphrase, kek);
    } catch {
      throw new Error('wrong passphrase');
    }
    await this.installAndUnwrap({
      masterKey: mk,
      publicKey: input.publicKey,
      encryptedPrivateKey: input.encryptedPrivateKey
    });
  }

  lock(): void {
    this.masterKey = null;
    this.publicKey = null;
    this.privateKey = null;
    this.stopIdleWatch();
    if (typeof window !== 'undefined') {
      window.sessionStorage.removeItem(STORAGE_KEY);
    }
  }

  private persist(): void {
    if (typeof window === 'undefined') return;
    if (!this.masterKey || !this.publicKey || !this.privateKey) return;
    const p: Persisted = {
      master_key: toBase64(this.masterKey),
      public_key: toBase64(this.publicKey),
      private_key: toBase64(this.privateKey)
    };
    window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(p));
    this.startIdleWatch();
  }

  /// Re-arm the idle timer + (idempotently) install activity listeners.
  /// Called whenever the vault becomes unlocked.
  private startIdleWatch(): void {
    if (typeof window === 'undefined') return;
    if (!this.idleHandler) {
      this.idleHandler = () => this.armTimer();
      for (const ev of IDLE_EVENTS) {
        document.addEventListener(ev, this.idleHandler, { passive: true });
      }
    }
    this.armTimer();
  }

  private stopIdleWatch(): void {
    if (this.idleTimer) {
      clearTimeout(this.idleTimer);
      this.idleTimer = null;
    }
    if (this.idleHandler && typeof window !== 'undefined') {
      for (const ev of IDLE_EVENTS) {
        document.removeEventListener(ev, this.idleHandler);
      }
      this.idleHandler = null;
    }
  }

  private armTimer(): void {
    if (this.idleTimer) clearTimeout(this.idleTimer);
    if (!this.isUnlocked) return;
    this.idleTimer = setTimeout(() => this.lock(), IDLE_LOCK_MS);
  }
}

export const vault = new Vault();
