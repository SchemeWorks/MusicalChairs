// IMPL-46d: the CLOSING path against the real program on a local validator started from
// the exported closing world (`local_cluster export-closing-fixture`, docs/solana/local-cluster.md):
// one 1-SOL Simple position opened ten days before wall time over a 0.1-SOL pot (a labelled
// synthetic fixture built by the LiteSVM World with the production ELF), so the owner's payout
// is insolvent today. Skipped unless MC_LOCAL_RPC (loopback only) and MC_LOCAL_OWNER_KEYPAIR
// (the exported owner secret) are set; the local `local_cluster oracle` must be running.
//
//   MC_LOCAL_RPC=http://127.0.0.1:8899 MC_LOCAL_OWNER_KEYPAIR=<keys>/owner.json \
//     npx vitest run frontend/src/solana/native/localClusterClosing.test.ts
//
// Flow: provision CloseRecord 1 -> insolvent withdraw_simple (closes the round) -> candidate pages
// -> build_candidates until sealed -> request_round_draw (ORAO) -> oracle fulfils -> capture ->
// apply_promotion -> finalize_round (round 2) -> participant page round 2 -> open_position in round 2.
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Connection, Keypair, LAMPORTS_PER_SOL, type Transaction } from '@solana/web3.js';

import { createNativeClient, type NativeClient } from './client';
import type { NativeWallet } from './wallet';
import { DRAW_STATE, type NativeTxResult } from './types';

const RPC = process.env.MC_LOCAL_RPC;
const OWNER_KEYPAIR = process.env.MC_LOCAL_OWNER_KEYPAIR;

function assertLoopback(url: string): void {
  const host = new URL(url).hostname;
  if (!['127.0.0.1', 'localhost', '[::1]', '::1'].includes(host)) {
    throw new Error(`MC_LOCAL_RPC must be a loopback validator, got ${host}`);
  }
}

function keypairWallet(keypair: Keypair): NativeWallet {
  return {
    get publicKey() {
      return keypair.publicKey.toBase58();
    },
    async signTransaction(tx: Transaction) {
      tx.partialSign(keypair);
      return tx;
    },
  };
}

async function finalized(result: NativeTxResult): Promise<string> {
  for await (const update of result.status$) {
    if (update.status === 'failed' || update.status === 'expired') {
      throw new Error(`${result.signature}: ${update.status} ${JSON.stringify(update.error, (_k, v) => (typeof v === 'bigint' ? v.toString() : v))}`);
    }
    if (update.status === 'finalized' || update.status === 'confirmed') return update.status;
  }
  return result.status;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe.skipIf(!RPC || !OWNER_KEYPAIR)('native adapter closes and reopens a round on the local cluster', () => {
  it('insolvent payout -> candidates -> randomized selection -> promotion -> finalize -> open in the next round', async () => {
    assertLoopback(RPC!);
    const connection = new Connection(RPC!, 'confirmed');
    const owner = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(OWNER_KEYPAIR!, 'utf8')) as number[]));
    const sig = await connection.requestAirdrop(owner.publicKey, 5 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction({ signature: sig, ...(await connection.getLatestBlockhash()) }, 'confirmed');
    const client: NativeClient = createNativeClient({ connection, wallet: keypairWallet(owner), sessionPubkey: owner.publicKey.toBase58() });

    const pool0 = (await client.readPool())!;
    expect(pool0.participantCount).toBe(1n);
    expect(pool0.pot).toBe(100_000_000n);
    const round = pool0.roundId;
    const closeId = pool0.nextCloseId;
    const position = (await client.readPosition(owner.publicKey.toBase58(), 1n))!;
    expect(position.principal).toBe(BigInt(LAMPORTS_PER_SOL));

    // 1. The CloseRecord must exist before a closing payout; the owner provisions it (caller-funded).
    await finalized(await client.provisionCloseRecord({ closeId, funding: 'Caller' }));
    expect(await client.readCloseRecord(closeId)).not.toBeNull();

    // 2. Ten days of accrual on 1 SOL (~1.1 SOL owed) against a 0.1-SOL pot: the payout is insolvent
    //    and closes the round; the owner receives the pot net of Carried Interest.
    const ownerBefore = await connection.getBalance(owner.publicKey);
    const payout = await client.withdrawSimple({ nonce: 1n, minimumReceive: 0n, maxWorkContribution: 0n });
    await finalized(payout);
    const close1 = (await client.readCloseRecord(closeId))!;
    expect(close1.participantCount).toBe(1n);
    const pool1 = (await client.readPool())!;
    expect(pool1.currentCloseId).toBe(closeId);
    expect(pool1.pot).toBe(0n);
    expect(await connection.getBalance(owner.publicKey)).toBeGreaterThan(ownerBefore);

    // 3. Candidate pages for both sets, then build_candidates until the digests are sealed.
    for (const setTag of [0, 1]) {
      for (let index = 0n; index < BigInt(Math.max(1, Number(close1.requiredPagesPerSet))); index += 1n) {
        await finalized(await client.provisionPage({ page: { kind: 'Candidate', closeId, setTag }, index, funding: 'Caller' }));
      }
    }
    let close = (await client.readCloseRecord(closeId))!;
    let builds = 0;
    while (!close.sealed && builds < 6) {
      await finalized(await client.buildCandidates({ closeId, rows: 8 }));
      close = (await client.readCloseRecord(closeId))!;
      builds += 1;
    }
    expect(close.sealed).toBe(true);
    expect(close.preferredCount + close.fallbackCount).toBeGreaterThan(0n);

    // 4. Randomness: request through ORAO, the local oracle fulfils, capture.
    await finalized(await client.requestDraw({ closeId }));
    close = (await client.readCloseRecord(closeId))!;
    expect(close.drawState).toBe(DRAW_STATE.Requested);
    let captured = false;
    for (let attempt = 0; attempt < 20 && !captured; attempt += 1) {
      await sleep(2_000);
      try {
        await finalized(await client.captureDraw({ closeId }));
        captured = true;
      } catch (error) {
        if (!String(error).includes('6043') && !String(error).includes('RequestNotFulfilled')) throw error;
      }
    }
    close = (await client.readCloseRecord(closeId))!;
    expect(captured).toBe(true);
    expect(close.drawState).toBe(DRAW_STATE.Captured);

    // 5. Promotion (the only candidate is the closing owner), then finalize: round + 1.
    await finalized(await client.applyPromotion({ closeId }));
    await finalized(await client.finalizeRound({ closeId }));
    const pool2 = (await client.readPool())!;
    expect(pool2.roundId).toBe(round + 1n);
    expect(pool2.currentCloseId).toBeNull();
    const closeFinal = (await client.readCloseRecord(closeId))!;
    expect(closeFinal.finalized).toBe(true);

    // 6. The next round opens: participant page 0 of the new round, then a fresh position.
    await finalized(await client.provisionPage({ page: { kind: 'Participant', round: pool2.roundId }, index: 0n, funding: 'Caller' }));
    const open = await client.openPosition({ plan: 'Simple', gross: 100_000_000n });
    await finalized(open);
    const reopened = await client.readPosition(owner.publicKey.toBase58(), open.nonce!);
    expect(reopened).not.toBeNull();
    expect(reopened!.roundId).toBe(pool2.roundId);
    console.log(`LOCAL_CLUSTER_CLOSE payout=${payout.signature} builds=${builds} promoted=${JSON.stringify(closeFinal.promotion, (_k, v) => (typeof v === 'bigint' ? v.toString() : v))} round=${pool2.roundId} reopen=${open.signature}`);
  }, 240_000);
});
