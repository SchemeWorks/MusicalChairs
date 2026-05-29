import { useMemo } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { ponziMathSolIdlFactory, PonziMathSolService } from '../backend';

// ponzi_math_sol canister ID. Matches the constant in usePonziMathSolActor.ts.
const PONZI_MATH_SOL_CANISTER_ID = 'spc6q-xyaaa-aaaac-qg2ma-cai';
const HOST = 'https://icp0.io';

let cachedActor: ActorSubclass<PonziMathSolService> | null = null;

export function useReadPonziMathSol(): ActorSubclass<PonziMathSolService> {
  return useMemo(() => {
    if (cachedActor) return cachedActor;
    const agent = new HttpAgent({ host: HOST });
    cachedActor = Actor.createActor<PonziMathSolService>(ponziMathSolIdlFactory, {
      agent,
      canisterId: PONZI_MATH_SOL_CANISTER_ID,
    });
    return cachedActor;
  }, []);
}
