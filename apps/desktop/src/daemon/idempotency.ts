/**
 * Idempotency keys for logical mutations.
 *
 * A key is minted per logical request and bound to a fingerprint of its
 * parameters. It is reused only while the previous outcome is unknown
 * (timeout, disconnect, in-progress, internal, protocol) and the parameters
 * are unchanged. Any definitive outcome, or a changed form, mints a new key.
 * Task ids derive from the key, so reuse cannot create a duplicate task.
 */
import type { DaemonError } from "./errors";

export class IdempotencyTracker {
  private readonly entries = new Map<string, { fingerprint: string; key: string }>();

  constructor(private readonly mint: () => string = () => crypto.randomUUID()) {}

  keyFor(scope: string, fingerprint: string): string {
    const existing = this.entries.get(scope);
    if (existing && existing.fingerprint === fingerprint) return existing.key;
    const key = this.mint();
    this.entries.set(scope, { fingerprint, key });
    return key;
  }

  /** Call with the outcome; the key survives only an indeterminate failure. */
  settle(scope: string, error: DaemonError | null): void {
    if (error === null || !error.indeterminate) this.entries.delete(scope);
  }

  /** Whether a retry of this scope would reuse an earlier key. */
  pending(scope: string): boolean {
    return this.entries.has(scope);
  }
}
