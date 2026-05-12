import { useMemo } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { ponziMathIdlFactory, PonziMathService } from '../backend';

// ponzi_math canister ID. Replace with the actual mainnet canister ID after
// ponzi_math is deployed. For local dev, replace with the locally-deployed ID.
const PONZI_MATH_CANISTER_ID = 'REPLACE_WITH_PONZI_MATH_CANISTER_ID';
const HOST = 'https://icp0.io';

let cachedActor: ActorSubclass<PonziMathService> | null = null;

export function useReadPonziMath(): ActorSubclass<PonziMathService> {
  return useMemo(() => {
    if (cachedActor) return cachedActor;
    const agent = new HttpAgent({ host: HOST });
    cachedActor = Actor.createActor<PonziMathService>(ponziMathIdlFactory, {
      agent,
      canisterId: PONZI_MATH_CANISTER_ID,
    });
    return cachedActor;
  }, []);
}
