// IMPL-47d: bind the local closing-fixture owner to a principal on the LOCAL siws_provider
// (the real ic-siws v0.1.0 wasm deployed at its pinned id on a dfx replica), so the M12
// Shenanigans `bindAndReserve` bound path can be exercised end to end. Skipped unless
// MC_LOCAL_IC (a loopback replica URL) and MC_LOCAL_OWNER_KEYPAIR are set; it signs a SIWS
// message with the synthetic fixture key only and talks only to that loopback replica.
//
//   MC_LOCAL_IC=http://127.0.0.1:4943 MC_LOCAL_OWNER_KEYPAIR=<keys>/owner.json \
//     npx vitest run frontend/src/solana/native/localSiwsLogin.test.ts
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Keypair } from '@solana/web3.js';
import bs58 from 'bs58';
import { Actor, HttpAgent } from '@dfinity/agent';
import { Ed25519KeyIdentity } from '@dfinity/identity';
import { Principal } from '@dfinity/principal';

import { idlFactory } from '../../declarations/siws_provider';
import { buildSiwsMessageText } from '../../lib/siwsSigner';

const IC = process.env.MC_LOCAL_IC;
const OWNER_KEYPAIR = process.env.MC_LOCAL_OWNER_KEYPAIR;
const SIWS_PROVIDER = 'tcm26-yqaaa-aaaac-qg2lq-cai';
const show = (v: unknown) => JSON.stringify(v, (_k, x) => (typeof x === 'bigint' ? x.toString() : x));

function assertLoopback(url: string): void {
  const host = new URL(url).hostname;
  if (!['127.0.0.1', 'localhost', '[::1]', '::1'].includes(host)) throw new Error(`MC_LOCAL_IC must be a loopback replica, got ${host}`);
}

describe.skipIf(!IC || !OWNER_KEYPAIR)('local SIWS login binds the fixture owner to a principal', () => {
  it('siws_prepare_login → sign → siws_login → get_principal resolves on the local replica', async () => {
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
  }, 60_000);
});
