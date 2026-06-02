// Pure display helpers for the Exit Liquidity game. No canister/odds logic
// lives here — the rotation hazard is server-side and never surfaced.

/** Format an integer-bps multiplier (10000 = 1.0x) as "1.60×". */
export function bpsToMultiplier(bps: bigint | number): string {
  const n = typeof bps === 'bigint' ? Number(bps) : bps;
  return `${(n / 10000).toFixed(2)}×`;
}

/**
 * Qualitative volatility tell. Coarse on purpose: it is the readable signal,
 * NOT the probability. Maps the current stage onto an escalating tier.
 */
export const TELL_TIERS = ['Calm', 'Firm', 'Choppy', 'Toppy', 'Critical'] as const;
export type TellTier = (typeof TELL_TIERS)[number];

export function tellForStage(stage: number, stageCount: number): TellTier {
  const last = TELL_TIERS.length - 1;
  if (stageCount <= 1) return TELL_TIERS[last];
  const frac = (stage - 1) / (stageCount - 1);
  const idx = Math.max(0, Math.min(last, Math.round(frac * last)));
  return TELL_TIERS[idx];
}

/** Project riding growth for the "if you survive" reward preview (reward is safe to show). */
export function nextStageRewardBps(ridingBps: number, stageStepBps: number): number {
  return Math.floor((ridingBps * stageStepBps) / 10000);
}
