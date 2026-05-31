import { useMemo, useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';
import { idlFactory } from '../declarations/shenanigans';
import type { _SERVICE } from '../declarations/shenanigans';
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';

// Shenanigans canister ID - mainnet deployment
const SHENANIGANS_CANISTER_ID = 'j56tm-oaaaa-aaaac-qf34q-cai';

// Host configuration - default to mainnet
const HOST = 'https://icp0.io';

interface UseShenaniganActorResult {
  actor: ActorSubclass<_SERVICE> | null;
  isFetching: boolean;
  error: Error | null;
}

let cachedReadActor: ActorSubclass<_SERVICE> | null = null;

/**
 * Anonymous read-only actor for shenanigans queries.
 *
 * MUST be used for any query hook (getShenaniganStats, getRecentShenanigans,
 * getReferralStats, etc.). For Oisy users the auth actor (`useShenaniganActor`)
 * is a SignerAgent that upgrades query calls to update calls via icrc49 —
 * which (a) opens the Oisy popup and (b) fails entirely until shenanigans
 * implements ICRC-21. Anonymous queries bypass the signer.
 */
export function useReadShenaniganActor(): ActorSubclass<_SERVICE> {
  return useMemo(() => {
    if (cachedReadActor) return cachedReadActor;
    const agent = new HttpAgent({ host: HOST });
    cachedReadActor = Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId: SHENANIGANS_CANISTER_ID,
    });
    return cachedReadActor;
  }, []);
}

export function useShenaniganActor(): UseShenaniganActorResult {
  const { identity, isInitializing, walletType, principal } = useWallet();
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

        // For Oisy, create actor via SignerAgent so update calls are signed.
        // Queries are still anonymous via the SignerAgent's internal HttpAgent.
        if (walletType === 'oisy' && principal) {
          const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
          const newActor = createOisyActor(SHENANIGANS_CANISTER_ID, idlFactory, signerAgent);
          setActor(newActor);
          return;
        }

        // For II / SIWS, the authed actor REQUIRES the delegation identity.
        // During the connect window `principal` can be set before `identity`
        // propagates; building an anonymous actor here makes auto-fired update
        // calls (e.g. registerReferral) trap "Anonymous principal not allowed".
        // Leave the actor null until the identity is ready — consumers gate on
        // a non-null actor. (Mirrors useAuthPpLedger.)
        if (!identity) {
          setActor(null);
          return;
        }
        const agent = new HttpAgent({
          host: HOST,
          identity,
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
  }, [identity, isInitializing, walletType, principal]);

  return { actor, isFetching, error };
}

export default useShenaniganActor;
