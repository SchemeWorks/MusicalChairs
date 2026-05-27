/**
 * Canister error prettifier.
 *
 * IC trap errors come back as a wall of text:
 *   Call failed: Canister: j56tm-... Method: castShenanigan (update) "Request ID": "..."
 *   "Error code": "IC0503" "Reject code": "5" "Reject message": "Error from Canister
 *   j56tm-...: Canister called `ic0.trap` with message: 'Insufficient chips to cast
 *   this shenanigan'.\nConsider gracefully handling failures from this canister or
 *   altering the canister to handle exceptions. See documentation: ..."
 *
 * The user only needs the bit between `with message: '` and the trailing `'`. The
 * rest is debug junk meaningful to canister authors but actively confusing to
 * players. This module extracts that needle and classifies common patterns so UI
 * can render contextual CTAs (e.g. "Deposit PP →" on insufficient_chips).
 */

export type ErrorKind =
  | 'insufficient_chips'
  | 'insufficient_pp_balance'
  | 'cooldown'
  | 'rate_limit'
  | 'rejected_target'
  | 'unknown';

export interface PrettyError {
  /** The short human-readable line to show as the headline. */
  message: string;
  /** Classification — UI uses this to decide whether to show contextual CTAs. */
  kind: ErrorKind;
  /** The raw error string, kept for "Details" disclosure or debugging. */
  raw: string;
}

/** Pull the trap message out of an IC error blob. Falls back to the input itself. */
function extractTrapMessage(raw: string): string {
  // Pattern: `with message: 'TRAPMESSAGE'.\nConsider ...`
  const withMessage = raw.match(/with message:\s*'([^']+)'/);
  if (withMessage) return withMessage[1];

  // Pattern: `"Reject message": "..."` — sometimes the trap content is here instead.
  const rejectMessage = raw.match(/"Reject message":\s*"([^"]+)"/);
  if (rejectMessage) {
    // Reject message often itself contains the `with message: '...'` segment.
    const inner = rejectMessage[1].match(/with message:\s*'([^']+)'/);
    if (inner) return inner[1];
    return rejectMessage[1];
  }

  // Pattern: bare Motoko/Rust trap text without the IC envelope, e.g. just
  // "Insufficient chips to cast this shenanigan".
  return raw.trim();
}

/** Classify a (stripped) trap message into a known error kind. */
function classify(message: string): ErrorKind {
  const m = message.toLowerCase();
  if (m.includes('insufficient chips')) return 'insufficient_chips';
  if (m.includes('insufficient pp')) return 'insufficient_pp_balance';
  if (m.includes('insufficient funds') && m.includes('pp')) return 'insufficient_pp_balance';
  if (m.includes('cooldown')) return 'cooldown';
  if (m.includes('rate limit') || m.includes('too many')) return 'rate_limit';
  if (m.includes('target') && (m.includes('shielded') || m.includes('rejected') || m.includes('not allowed'))) {
    return 'rejected_target';
  }
  return 'unknown';
}

export function prettifyCanisterError(input: unknown): PrettyError {
  const raw =
    input instanceof Error
      ? input.message
      : typeof input === 'string'
        ? input
        : JSON.stringify(input ?? 'Unknown error');
  const message = extractTrapMessage(raw);
  return { message, kind: classify(message), raw };
}
