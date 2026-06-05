import { describe, it, expect } from 'vitest';
import { bpsToMultiplier, tellForStage, nextStageRewardBps, TELL_TIERS } from './exitLiquidity';

describe('bpsToMultiplier', () => {
  it('formats bps as an x-multiplier', () => {
    expect(bpsToMultiplier(10000)).toBe('1.00×');
    expect(bpsToMultiplier(16000)).toBe('1.60×');
    expect(bpsToMultiplier(0)).toBe('0.00×');
  });
  it('accepts bigint', () => {
    expect(bpsToMultiplier(25600n)).toBe('2.56×');
  });
});

describe('tellForStage', () => {
  it('ramps from calm to critical across the stages', () => {
    expect(tellForStage(1, 5)).toBe('Calm');
    expect(tellForStage(5, 5)).toBe('Critical');
  });
  it('clamps and handles a single-stage config', () => {
    expect(tellForStage(9, 5)).toBe('Critical');
    expect(tellForStage(1, 1)).toBe(TELL_TIERS[TELL_TIERS.length - 1]);
  });
});

describe('nextStageRewardBps', () => {
  it('projects riding growth for the reward preview', () => {
    expect(nextStageRewardBps(10000, 16000)).toBe(16000);
    expect(nextStageRewardBps(16000, 16000)).toBe(25600);
  });
});
