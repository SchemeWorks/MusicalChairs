// IMPL-47d: bind the local closing-fixture owner to a principal on the LOCAL siws_provider
// (the real ic-siws v0.1.0 wasm deployed at its pinned id on a dfx replica), so the M12
// Shenanigans `bindAndReserve` bound path can be exercised end to end. Skipped unless
// MC_LOCAL_IC (a loopback replica URL) and MC_LOCAL_OWNER_KEYPAIR are set; it signs a SIWS
// message with the synthetic fixture key only and talks only to that loopback replica.
//
// The full login is exercised the way the app does it (siwsSigner.connectSiws): prepare →
// sign → login → get_delegation → DelegationChain, and the DelegationIdentity's principal must
// equal the canister's `get_principal(address)`. With MC_LOCAL_SESSION_OUT set, the resulting
// session is written in the app's localStorage format (the three `musical-chairs-siws-*` keys +
// wallet type) so a dev-server browser session can be restored from it for the legacy-rail
// checks without a browser-extension wallet. The file holds a session key for the LOCAL replica
// only; it is written outside the repository.
//
//   MC_LOCAL_IC=http://127.0.0.1:4943 MC_LOCAL_OWNER_KEYPAIR=<keys>/owner.json \
//     [MC_LOCAL_SESSION_OUT=<keys>/browser-session.json] \
//     npx vitest run frontend/src/solana/native/localSiwsLogin.test.ts
import { readFileSync, writeFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Keypair } from '@solana/web3.js';
import bs58 from 'bs58';
import { Actor, HttpAgent } from '@dfinity/agent';
import { DelegationIdentity, Ed25519KeyIdentity } from '@dfinity/identity';
import { Principal } from '@dfinity/principal';

import { idlFactory } from '../../declarations/siws_provider';
import { buildDelegationChain, buildSiwsMessageText } from '../../lib/siwsSigner';

const IC = process.env.MC_LOCAL_IC;
const OWNER_KEYPAIR = process.env.MC_LOCAL_OWNER_KEYPAIR;
const SESSION_OUT = process.env.MC_LOCAL_SESSION_OUT;
const SIWS_PROVIDER = 'tcm26-yqaaa-aaaac-qg2lq-cai';
const show = (v: unknown) => JSON.stringify(v, (_k, x) => (typeof x === 'bigint' ? x.toString() : x));

function assertLoopback(url: string): void {
  const host = new URL(url).hostname;
  if (!['127.0.0.1', 'localhost', '[::1]', '::1'].includes(host)) throw new Error(`MC_LOCAL_IC must be a loopback replica, got ${host}`);
}

describe.skipIf(!IC || !OWNER_KEYPAIR)('local SIWS login binds the fixture owner to a principal', () => {
  it('siws_prepare_login → sign → siws_login → get_delegation yields the principal get_principal reports', async () => {
    assertLoopback(IC!);
    const owner = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(OWNER_KEYPAIR!, 'utf8')) as number[]));
    const address = owner.publicKey.toBase58();
    const agent = await HttpAgent.create({ host: IC!, shouldFetchRootKey: true });
    const actor = Actor.createActor(idlFactory, { agent, canisterId: SIWS_PROVIDER }) as any;

    const before = await actor.get_principal(address);
    console.log('before:', show(before));

    const prepared = await actor.siws_prepare_login(address);
    expect('Ok' in prepared, show(prepared)).toBe(true);
    const message = prepared.Ok;
    const bytes = new TextEncoder().encode(buildSiwsMessageText(message));
    // Sign with the same Ed25519 key through @dfinity/identity (a direct dependency; tweetnacl is not).
    const signer = Ed25519KeyIdentity.fromSecretKey(owner.secretKey.slice(0, 32));
    const signature = new Uint8Array(await signer.sign(bytes));
    const session = Ed25519KeyIdentity.generate();
    const sessionKey = new Uint8Array(session.getPublicKey().toDer());
    const login = await actor.siws_login(bs58.encode(signature), address, sessionKey, message.nonce);
    expect('Ok' in login, show(login)).toBe(true);

    const after = await actor.get_principal(address);
    expect('Ok' in after, show(after)).toBe(true);
    const principal = Principal.fromUint8Array(new Uint8Array(after.Ok)).toText();
    console.log(`LOCAL_SIWS address=${address} principal=${principal}`);
    expect(principal.length).toBeGreaterThan(10);

    // The delegation the app would build: its identity must resolve to the same principal.
    const delegation = await actor.siws_get_delegation(address, sessionKey, login.Ok.expiration);
    expect('Ok' in delegation, show(delegation)).toBe(true);
    const chain = buildDelegationChain(delegation.Ok, login.Ok.user_canister_pubkey);
    const identity = DelegationIdentity.fromDelegation(session, chain);
    expect(identity.getPrincipal().toText()).toBe(principal);
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    expect(chain.delegations.every((d) => d.delegation.expiration > nowNs)).toBe(true);

    if (SESSION_OUT) {
      // Exactly what siwsSigner.connectSiws persists, so restoreSiwsSession() can rehydrate it.
      const out = {
        'musical-chairs-wallet-type': 'siws',
        'musical-chairs-siws-session-key': JSON.stringify(session.toJSON()),
        'musical-chairs-siws-delegation': JSON.stringify(chain.toJSON()),
        'musical-chairs-siws-pubkey': address,
        principal,
        replica: IC,
      };
      writeFileSync(SESSION_OUT, JSON.stringify(out, null, 2));
      console.log(`LOCAL_SIWS session written to ${SESSION_OUT}`);
    }
  }, 60_000);
});
