import { useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { useWallet } from './useWallet';
import { idlFactory } from '../declarations/backend';
import type { _SERVICE } from '../declarations/backend';

// Backend canister ID - mainnet deployment
const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';

// Host configuration - default to mainnet
const HOST = 'https://icp0.io';

interface UseActorResult {
  actor: ActorSubclass<_SERVICE> | null;
  isFetching: boolean;
  error: Error | null;
}

export function useActor(): UseActorResult {
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

        // For Plug wallet, we need to use Plug's agent
        if (walletType === 'plug' && window.ic?.plug?.agent) {
          // Plug provides its own agent - create actor with it
          const plugAgent = window.ic.plug.agent as any;
          
          const newActor = Actor.createActor<_SERVICE>(idlFactory, {
            agent: plugAgent,
            canisterId: BACKEND_CANISTER_ID,
          });

          setActor(newActor);
          return;
        }

        // For II/OISY or anonymous, create standard HTTP agent
        const agent = new HttpAgent({
          host: HOST,
          identity: identity || undefined,
        });

        // Fetch root key for local development (not needed for mainnet)
        if (process.env.DFX_NETWORK !== 'ic') {
          try {
            await agent.fetchRootKey();
          } catch (err) {
            console.warn('Unable to fetch root key. Is the local replica running?');
          }
        }

        // Create actor
        const newActor = Actor.createActor<_SERVICE>(idlFactory, {
          agent,
          canisterId: BACKEND_CANISTER_ID,
        });

        setActor(newActor);
      } catch (err) {
        console.error('Failed to create actor:', err);
        setError(err instanceof Error ? err : new Error('Failed to create actor'));
      } finally {
        setIsFetching(false);
      }
    };

    createActor();
  }, [identity, isInitializing, walletType]);

  return { actor, isFetching, error };
}

export default useActor;
