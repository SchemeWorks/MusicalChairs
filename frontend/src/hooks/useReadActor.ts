import { useMemo } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { idlFactory } from '../declarations/backend';
import type { _SERVICE } from '../declarations/backend';

const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';
const HOST = 'https://icp0.io';

let cachedActor: ActorSubclass<_SERVICE> | null = null;

export function useReadActor(): ActorSubclass<_SERVICE> {
  return useMemo(() => {
    if (cachedActor) return cachedActor;
    const agent = new HttpAgent({ host: HOST });
    cachedActor = Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId: BACKEND_CANISTER_ID,
    });
    return cachedActor;
  }, []);
}
