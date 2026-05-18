import { useMemo, useState, useEffect } from 'react';
import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import { useWallet } from './useWallet';
import { idlFactory } from '../declarations/pp_ledger';
import type { _SERVICE } from '../declarations/pp_ledger';
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';

export const PP_LEDGER_CANISTER_ID = '5xv2o-iiaaa-aaaac-qeclq-cai';
const HOST = 'https://icp0.io';

const SHENANIGANS_CANISTER_ID = 'j56tm-oaaaa-aaaac-qf34q-cai';

function makeAnonActor(): ActorSubclass<_SERVICE> {
  const agent = new HttpAgent({ host: HOST });
  return Actor.createActor<_SERVICE>(idlFactory, {
    agent,
    canisterId: PP_LEDGER_CANISTER_ID,
  });
}

export function useReadPpLedger(): ActorSubclass<_SERVICE> {
  return useMemo(() => makeAnonActor(), []);
}

export function useAuthPpLedger(): ActorSubclass<_SERVICE> | null {
  const { identity, walletType, isInitializing, principal } = useWallet();
  const [actor, setActor] = useState<ActorSubclass<_SERVICE> | null>(null);

  useEffect(() => {
    if (isInitializing) return;

    let cancelled = false;

    (async () => {
      if (walletType === 'plug' && (window as any).ic?.plug?.agent) {
        const next = Actor.createActor<_SERVICE>(idlFactory, {
          agent: (window as any).ic.plug.agent,
          canisterId: PP_LEDGER_CANISTER_ID,
        });
        if (!cancelled) setActor(next);
        return;
      }

      if (walletType === 'oisy' && principal) {
        const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
        const next = createOisyActor(PP_LEDGER_CANISTER_ID, idlFactory, signerAgent);
        if (!cancelled) setActor(next);
        return;
      }

      if (!identity) {
        if (!cancelled) setActor(null);
        return;
      }

      const agent = new HttpAgent({ host: HOST, identity });
      const next = Actor.createActor<_SERVICE>(idlFactory, {
        agent,
        canisterId: PP_LEDGER_CANISTER_ID,
      });
      if (!cancelled) setActor(next);
    })().catch((err) => {
      console.error('Failed to create pp_ledger auth actor:', err);
      if (!cancelled) setActor(null);
    });

    return () => {
      cancelled = true;
    };
  }, [identity, walletType, isInitializing, principal]);

  return actor;
}

/** Build the 32-byte chip subaccount for a principal (mirrors shenanigans/Subaccount.mo). */
export function principalToChipSubaccount(principal: Principal): Uint8Array {
  const bytes = principal.toUint8Array();
  const out = new Uint8Array(32);
  out.set(bytes);
  return out;
}

export function shenanigansOwner(): Principal {
  return Principal.fromText(SHENANIGANS_CANISTER_ID);
}

export const PP_DECIMALS = 8;
export const PP_UNIT_SCALE = 100_000_000n;

/** Format PP-units as a whole-number string (no decimals shown by default). */
export function ppUnitsToWhole(units: bigint): number {
  return Number(units / PP_UNIT_SCALE);
}

/** Parse a whole-number PP amount into PP-units. */
export function wholePpToUnits(whole: number | bigint): bigint {
  if (typeof whole === 'number') return BigInt(Math.trunc(whole)) * PP_UNIT_SCALE;
  return whole * PP_UNIT_SCALE;
}
