import { useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';
import { ponziMathIdlFactory, PonziMathService } from '../backend';
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';

// ponzi_math canister ID. Replace with the actual mainnet canister ID after
// ponzi_math is deployed. For local dev, replace with the locally-deployed ID.
const PONZI_MATH_CANISTER_ID = 'guy42-yqaaa-aaaaj-qr5pq-cai';

const HOST = 'https://icp0.io';

interface UsePonziMathActorResult {
  actor: ActorSubclass<PonziMathService> | null;
  isFetching: boolean;
  error: Error | null;
}

export function usePonziMathActor(): UsePonziMathActorResult {
  const { identity, isInitializing, walletType, principal } = useWallet();
  const [actor, setActor] = useState<ActorSubclass<PonziMathService> | null>(null);
  const [isFetching, setIsFetching] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    if (isInitializing) {
      return;
    }

    const createActor = async () => {
      try {
        setIsFetching(true);
        setError(null);

        if (walletType === 'plug' && window.ic?.plug?.agent) {
          const plugAgent = window.ic.plug.agent as any;
          const newActor = Actor.createActor<PonziMathService>(ponziMathIdlFactory, {
            agent: plugAgent,
            canisterId: PONZI_MATH_CANISTER_ID,
          });
          setActor(newActor);
          return;
        }

        if (walletType === 'oisy' && principal) {
          const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
          const newActor = createOisyActor(PONZI_MATH_CANISTER_ID, ponziMathIdlFactory, signerAgent);
          setActor(newActor);
          return;
        }

        const agent = new HttpAgent({
          host: HOST,
          identity: identity || undefined,
        });

        if (process.env.DFX_NETWORK !== 'ic') {
          try {
            await agent.fetchRootKey();
          } catch (err) {
            console.warn('Unable to fetch root key. Is the local replica running?');
          }
        }

        const newActor = Actor.createActor<PonziMathService>(ponziMathIdlFactory, {
          agent,
          canisterId: PONZI_MATH_CANISTER_ID,
        });

        setActor(newActor);
      } catch (err) {
        console.error('Failed to create ponzi_math actor:', err);
        setError(err instanceof Error ? err : new Error('Failed to create ponzi_math actor'));
      } finally {
        setIsFetching(false);
      }
    };

    createActor();
  }, [identity, isInitializing, walletType, principal]);

  return { actor, isFetching, error };
}

export default usePonziMathActor;
