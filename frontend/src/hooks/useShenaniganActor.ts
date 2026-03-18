import { useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { useWallet } from './useWallet';
import { idlFactory } from '../declarations/shenanigans';
import type { _SERVICE } from '../declarations/shenanigans';

// Shenanigans canister ID - mainnet deployment
const SHENANIGANS_CANISTER_ID = 'j56tm-oaaaa-aaaac-qf34q-cai';

// Host configuration - default to mainnet
const HOST = 'https://icp0.io';

interface UseShenaniganActorResult {
  actor: ActorSubclass<_SERVICE> | null;
  isFetching: boolean;
  error: Error | null;
}

export function useShenaniganActor(): UseShenaniganActorResult {
  const { identity, isInitializing, walletType } = useWallet();
  const [actor, setActor] = useState<ActorSubclass<_SERVICE> | null>(null);
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

        // For Plug wallet, use Plug's agent
        if (walletType === 'plug' && window.ic?.plug?.agent) {
          const plugAgent = window.ic.plug.agent as any;

          const newActor = Actor.createActor<_SERVICE>(idlFactory, {
            agent: plugAgent,
            canisterId: SHENANIGANS_CANISTER_ID,
          });

          setActor(newActor);
          return;
        }

        // For II/OISY or anonymous, create standard HTTP agent
        const agent = new HttpAgent({
          host: HOST,
          identity: identity || undefined,
        });

        // Fetch root key for local development
        if (process.env.DFX_NETWORK !== 'ic') {
          try {
            await agent.fetchRootKey();
          } catch (err) {
            console.warn('Unable to fetch root key. Is the local replica running?');
          }
        }

        const newActor = Actor.createActor<_SERVICE>(idlFactory, {
          agent,
          canisterId: SHENANIGANS_CANISTER_ID,
        });

        setActor(newActor);
      } catch (err) {
        console.error('Failed to create shenanigans actor:', err);
        setError(err instanceof Error ? err : new Error('Failed to create shenanigans actor'));
      } finally {
        setIsFetching(false);
      }
    };

    createActor();
  }, [identity, isInitializing, walletType]);

  return { actor, isFetching, error };
}

export default useShenaniganActor;
