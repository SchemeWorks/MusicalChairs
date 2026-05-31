import { useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';
import { ponziMathSolIdlFactory, PonziMathSolService } from '../backend';
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';

// ponzi_math_sol canister ID. Mainnet RPC config lives on the canister itself.
const PONZI_MATH_SOL_CANISTER_ID = 'spc6q-xyaaa-aaaac-qg2ma-cai';

const HOST = 'https://icp0.io';

interface UsePonziMathSolActorResult {
  actor: ActorSubclass<PonziMathSolService> | null;
  isFetching: boolean;
  error: Error | null;
}

export function usePonziMathSolActor(): UsePonziMathSolActorResult {
  const { identity, isInitializing, walletType, principal } = useWallet();
  const [actor, setActor] = useState<ActorSubclass<PonziMathSolService> | null>(null);
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
          const newActor = Actor.createActor<PonziMathSolService>(ponziMathSolIdlFactory, {
            agent: plugAgent,
            canisterId: PONZI_MATH_SOL_CANISTER_ID,
          });
          setActor(newActor);
          return;
        }

        if (walletType === 'oisy' && principal) {
          const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
          const newActor = createOisyActor(PONZI_MATH_SOL_CANISTER_ID, ponziMathSolIdlFactory, signerAgent);
          setActor(newActor);
          return;
        }

        // Auth actor requires the delegation identity (II/SIWS); don't build an
        // anonymous actor during the connect window — auto-fired update calls
        // would trap. Leave it null until the identity is ready. (See
        // useShenaniganActor / useAuthPpLedger for the same guard.)
        if (!identity) {
          setActor(null);
          return;
        }
        const agent = new HttpAgent({
          host: HOST,
          identity,
        });

        if (process.env.DFX_NETWORK !== 'ic') {
          try {
            await agent.fetchRootKey();
          } catch (err) {
            console.warn('Unable to fetch root key. Is the local replica running?');
          }
        }

        const newActor = Actor.createActor<PonziMathSolService>(ponziMathSolIdlFactory, {
          agent,
          canisterId: PONZI_MATH_SOL_CANISTER_ID,
        });

        setActor(newActor);
      } catch (err) {
        console.error('Failed to create ponzi_math_sol actor:', err);
        setError(err instanceof Error ? err : new Error('Failed to create ponzi_math_sol actor'));
      } finally {
        setIsFetching(false);
      }
    };

    createActor();
  }, [identity, isInitializing, walletType, principal]);

  return { actor, isFetching, error };
}

export default usePonziMathSolActor;
