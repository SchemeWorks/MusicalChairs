// IMPL-46c: the confirmation ladder as the panels present it. `unknown` is never failure
// and never offers a rebuild; only failed / expired do.
import { describe, it, expect } from 'vitest';
import { describeTxStatus, furtherStatus, shortSignature } from './panelsStatus';

const SIG = '5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW';

describe('transaction status view', () => {
  it('submitted -> confirmed -> finalized never offers a retry or rebuild', () => {
    const submitted = describeTxStatus({ status: 'submitted' }, { label: 'Your deposit' });
    expect(submitted).toMatchObject({ tone: 'pending', terminal: false, canPollAgain: false, canRebuild: false, headline: 'Submitted' });
    const confirmed = describeTxStatus({ status: 'confirmed' }, { label: 'Your deposit' });
    expect(confirmed).toMatchObject({ tone: 'pending', terminal: false, canRebuild: false, headline: 'Confirmed' });
    const finalized = describeTxStatus({ status: 'finalized' }, { label: 'Your deposit' });
    expect(finalized).toMatchObject({ tone: 'success', terminal: true, canRebuild: false, headline: 'Finalized' });
  });

  it('no observation yet reads as submitted', () => {
    expect(describeTxStatus(null, { label: 'Your deposit' }).status).toBe('submitted');
  });

  it('unknown keeps the signature, offers "check again", and forbids a rebuild', () => {
    const v = describeTxStatus({ status: 'unknown' }, { label: 'Your deposit' });
    expect(v.tone).toBe('warning');
    expect(v.canPollAgain).toBe(true);
    expect(v.canRebuild).toBe(false);
    expect(v.terminal).toBe(false);
    expect(v.detail).toMatch(/signature is saved/i);
    expect(v.detail).toMatch(/send it twice/i);
  });

  it('failed shows the program error copy and allows a new transaction', () => {
    const v = describeTxStatus({ status: 'failed', error: { InstructionError: [0, { Custom: 6010 }] } }, { label: 'Your withdrawal' });
    expect(v).toMatchObject({ tone: 'danger', terminal: true, canRebuild: true, canPollAgain: false });
    expect(v.detail).toMatch(/resume processing/i); // AllocationPending
    const plain = describeTxStatus({ status: 'failed' }, { label: 'Your withdrawal' });
    expect(plain.detail).toMatch(/rejected by the cluster/);
  });

  it('expired is safe to retry (nothing landed)', () => {
    const v = describeTxStatus({ status: 'expired' }, { label: 'Your claim' });
    expect(v).toMatchObject({ terminal: true, canRebuild: true, canPollAgain: false });
    expect(v.detail).toMatch(/safe to try again/);
  });

  it('furtherStatus keeps the most advanced observation of a signature', () => {
    const unknown = { signature: SIG, status: 'unknown' as const };
    const confirmed = { signature: SIG, status: 'confirmed' as const };
    const finalized = { signature: SIG, status: 'finalized' as const };
    expect(furtherStatus(unknown, confirmed)).toBe(confirmed);
    expect(furtherStatus(finalized, confirmed)).toBe(finalized);
    expect(furtherStatus(null, confirmed)).toBe(confirmed);
    expect(furtherStatus(confirmed, null)).toBe(confirmed);
    expect(furtherStatus(null, null)).toBeNull();
  });

  it('shortens a signature for display and leaves a short one alone', () => {
    expect(shortSignature(SIG)).toBe(`${SIG.slice(0, 8)}…${SIG.slice(-8)}`);
    expect(shortSignature('abc')).toBe('abc');
  });
});
