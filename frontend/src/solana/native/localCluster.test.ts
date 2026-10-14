// IMPL-46d: the adapter against the REAL program on a local `solana-test-validator`
// (docs/solana/local-cluster.md). Skipped unless MC_LOCAL_RPC points at a running,
// bootstrapped local cluster — it is the only test here that signs and sends
// transactions, and it only ever talks to that local URL.
//
//   MC_LOCAL_RPC=http://127.0.0.1:8899 npx vitest run frontend/src/solana/native/localCluster.test.ts
//
// Flows: open (Simple, 1 SOL) -> dust payout (no Backers: toll seeds) -> Series A
// funding by a second wallet -> payout with a ticket-bearing toll -> the keeper
// phases through `advancePhase` until IDLE -> Backer claim. The closing / promotion
// path needs an insolvent payout, which takes days of real accrual; it stays
// proven in LiteSVM (M7/M8) and is not driven here.
import { describe, expect, it } from 'vitest';
import { Connection, Keypair, LAMPORTS_PER_SOL, type Transaction } from '@solana/web3.js';

import { createNativeClient, type NativeClient } from './client';
import type { NativeWallet } from './wallet';
import { TICKET_PHASE, type NativeTxResult } from './types';

const RPC = process.env.MC_LOCAL_RPC;

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

async function airdrop(connection: Connection, to: Keypair, sol: number) {
  const signature = await connection.requestAirdrop(to.publicKey, sol * LAMPORTS_PER_SOL);
  const latest = await connection.getLatestBlockhash();
  await connection.confirmTransaction({ signature, ...latest }, 'confirmed');
}

async function finalized(result: NativeTxResult): Promise<string> {
  for await (const update of result.status$) {
    if (update.status === 'failed' || update.status === 'expired') {
      throw new Error(`${result.signature}: ${update.status} ${JSON.stringify(update.error)}`);
    }
    if (update.status === 'finalized' || update.status === 'confirmed') return update.status;
  }
  return result.status;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe.skipIf(!RPC)('native adapter against the local cluster', () => {
  it('opens, pays out, funds Series A, runs the allocation ticket and claims through the real program', async () => {
    const connection = new Connection(RPC!, 'confirmed');
    const owner = Keypair.generate();
    const backer = Keypair.generate();
    await airdrop(connection, owner, 5);
    await airdrop(connection, backer, 5);
    const client: NativeClient = createNativeClient({ connection, wallet: keypairWallet(owner), sessionPubkey: owner.publicKey.toBase58() });
    const backerClient: NativeClient = createNativeClient({ connection, wallet: keypairWallet(backer), sessionPubkey: backer.publicKey.toBase58() });

    const pool0 = await client.readPool();
    expect(pool0).not.toBeNull();
    const round = pool0!.roundId;

    // 1. Open a 1-SOL Simple position: 4 % Front-End Load to management, 96 % to the pot.
    const open = await client.openPosition({ plan: 'Simple', gross: BigInt(LAMPORTS_PER_SOL) });
    expect(open.nonce).not.toBeNull();
    await finalized(open);
    const position = await client.readPosition(owner.publicKey.toBase58(), open.nonce!);
    expect(position).not.toBeNull();
    expect(position!.principal).toBe(BigInt(LAMPORTS_PER_SOL));
    const pool1 = (await client.readPool())!;
    expect(pool1.pot - pool0!.pot).toBe(BigInt(960_000_000));
    expect(pool1.management - pool0!.management).toBe(BigInt(40_000_000));
    expect(pool1.roundId).toBe(round);

    // 2. A payout a few seconds later: owed ~12.7 lamports/s on 1 SOL; with an empty registry the
    //    Carried Interest goes straight to the seed and no ticket starts.
    await sleep(4_000);
    const payout1 = await client.withdrawSimple({ nonce: open.nonce!, minimumReceive: 0n, maxWorkContribution: 0n });
    await finalized(payout1);
    const pool2 = (await client.readPool())!;
    expect(pool2.seed).toBeGreaterThan(pool1.seed);
    expect(pool2.activeTicket).toBe(0n);

    // 3. Series A funding by a second wallet (minimum 0.05 SOL; entitlement 124 %).
    const fund = await backerClient.fundSeriesA({ gross: BigInt(50_000_000) });
    await finalized(fund);
    const backerAccount = await backerClient.readBacker(backer.publicKey.toBase58(), 0);
    expect(backerAccount).not.toBeNull();
    expect(backerAccount!.entitlement).toBe(BigInt(62_000_000));
    const pool3 = (await client.readPool())!;
    expect(pool3.registryLen).toBe(1n);

    // 4. A payout with a ticket-bearing toll (>= 5 lamports) starts the allocation run; the keeper
    //    phases run through advancePhase until the ticket is IDLE again.
    await sleep(6_000);
    const payout2 = await client.withdrawSimple({ nonce: open.nonce!, minimumReceive: 0n, maxWorkContribution: 0n });
    await finalized(payout2);
    let ticket = (await client.readTicket())!;
    const started = ticket.phase !== TICKET_PHASE.Idle;
    let steps = 0;
    while (ticket.phase !== TICKET_PHASE.Idle && steps < 12) {
      const step = await client.advancePhase({ rows: 8 });
      await finalized(step);
      ticket = (await client.readTicket())!;
      steps += 1;
    }
    expect(ticket.phase).toBe(TICKET_PHASE.Idle);
    const pool4 = (await client.readPool())!;
    expect(pool4.pendingToll).toBe(0n);
    if (started) expect(pool4.fundedClaims).toBeGreaterThan(0n);

    // 5. The Backer claims its finalized credit.
    if (pool4.fundedClaims > 0n) {
      const claim = await backerClient.claimBacker({});
      await finalized(claim);
      const pool5 = (await client.readPool())!;
      expect(pool5.fundedClaims).toBe(0n);
    }
    console.log(`LOCAL_CLUSTER open=${open.signature} payout1=${payout1.signature} fund=${fund.signature} payout2=${payout2.signature} ticketStarted=${started} steps=${steps}`);
  }, 180_000);
});
