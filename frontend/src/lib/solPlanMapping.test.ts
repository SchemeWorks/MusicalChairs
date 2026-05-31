import { describe, it, expect } from 'vitest';
import { investPlanToSolGamePlan, ppPerSolForPlan } from './solPlanMapping';
import { SolGamePlan } from '../backend';
import {
  PP_PER_SOL_SIMPLE,
  PP_PER_SOL_COMPOUND_15,
  PP_PER_SOL_COMPOUND_30,
} from './gameConstants';

describe('investPlanToSolGamePlan', () => {
  it('maps 21-day-simple → simple21Day', () => {
    expect(investPlanToSolGamePlan('21-day-simple')).toEqual(SolGamePlan.simple21Day);
  });
  it('maps 15-day-compounding → compounding15Day', () => {
    expect(investPlanToSolGamePlan('15-day-compounding')).toEqual(SolGamePlan.compounding15Day);
  });
  it('maps 30-day-compounding → compounding30Day', () => {
    expect(investPlanToSolGamePlan('30-day-compounding')).toEqual(SolGamePlan.compounding30Day);
  });
  it('falls back to simple21Day for an unknown id', () => {
    expect(investPlanToSolGamePlan('nonsense')).toEqual(SolGamePlan.simple21Day);
  });
});

describe('ppPerSolForPlan', () => {
  it('returns the simple rate', () => {
    expect(ppPerSolForPlan('21-day-simple')).toBe(PP_PER_SOL_SIMPLE);
  });
  it('returns the 15-day rate', () => {
    expect(ppPerSolForPlan('15-day-compounding')).toBe(PP_PER_SOL_COMPOUND_15);
  });
  it('returns the 30-day rate', () => {
    expect(ppPerSolForPlan('30-day-compounding')).toBe(PP_PER_SOL_COMPOUND_30);
  });
  it('falls back to the simple rate for an unknown id', () => {
    expect(ppPerSolForPlan('nonsense')).toBe(PP_PER_SOL_SIMPLE);
  });
});
