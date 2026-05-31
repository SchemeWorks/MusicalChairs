import { SolGamePlan } from '../backend';
import {
  PP_PER_SOL_SIMPLE,
  PP_PER_SOL_COMPOUND_15,
  PP_PER_SOL_COMPOUND_30,
} from './gameConstants';

// Invest-tab plan id (as used by getDailyRate/getPlanDays) → ponzi_math_sol
// GamePlan variant. Unknown ids fall back to the simple plan defensively.
export function investPlanToSolGamePlan(planId: string): SolGamePlan {
  switch (planId) {
    case '15-day-compounding':
      return SolGamePlan.compounding15Day;
    case '30-day-compounding':
      return SolGamePlan.compounding30Day;
    case '21-day-simple':
    default:
      return SolGamePlan.simple21Day;
  }
}

// Display-only PP-per-SOL rate for an invest-tab plan id.
export function ppPerSolForPlan(planId: string): number {
  switch (planId) {
    case '15-day-compounding':
      return PP_PER_SOL_COMPOUND_15;
    case '30-day-compounding':
      return PP_PER_SOL_COMPOUND_30;
    case '21-day-simple':
    default:
      return PP_PER_SOL_SIMPLE;
  }
}
