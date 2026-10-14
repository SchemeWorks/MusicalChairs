// Keep the main bundle free of @solana/web3.js (rpc.ts explains why): modules the
// main chunk may import statically must not value-import web3.js, and the hook
// file must reach the web3.js-using modules only through dynamic import().
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

const read = (relative: string) => readFileSync(new URL(relative, import.meta.url), 'utf8');

/** Import statements that load a module at runtime (everything except `import type ...`). */
function staticValueImports(source: string): string[] {
  const statements = source.match(/^import\s[\s\S]*?from\s+['"][^'"]+['"];?/gm) ?? [];
  return statements.filter((s) => !/^import\s+type\s/.test(s)).map((s) => /from\s+['"]([^'"]+)['"]/.exec(s)![1]);
}

const WEB3_FREE = ['codecs.ts', 'types.ts', 'status.ts', 'sampling.ts', 'selectors.ts', 'economics.ts', 'wallet.ts'];
const WEB3_USING = ['accounts.ts', 'transactions.ts', 'client.ts'];

describe('bundle hygiene', () => {
  it('web3-free native modules never value-import @solana/web3.js or the web3-using modules', () => {
    for (const file of WEB3_FREE) {
      const imports = staticValueImports(read(`./${file}`));
      expect(imports.filter((i) => i.startsWith('@solana/')), file).toEqual([]);
      expect(imports.filter((i) => /\/(accounts|transactions|client)$/.test(i)), file).toEqual([]);
    }
  });

  it('the web3-using modules are exactly the ones the hook loads lazily', () => {
    for (const file of WEB3_USING) {
      expect(staticValueImports(read(`./${file}`)).some((i) => i === '@solana/web3.js'), file).toBe(true);
    }
  });

  it('hooks/useNativeSolana.ts imports web3-free modules statically and everything else dynamically', () => {
    const source = read('../../hooks/useNativeSolana.ts');
    const imports = staticValueImports(source);
    expect(imports.filter((i) => i.startsWith('@solana/'))).toEqual([]);
    expect(imports.filter((i) => /solana\/native\/(accounts|transactions|client|wallet|testFixtures)$/.test(i))).toEqual([]);
    expect(source).toMatch(/import\('\.\.\/solana\/native\/client'\)/);
    expect(source).toMatch(/import\('\.\.\/solana\/native\/wallet'\)/);
    expect(source).toMatch(/import\('\.\.\/solana\/native\/accounts'\)/);
  });

  it('adds no dependency: package.json carries no rollup platform package and no anchor client', () => {
    const pkg = JSON.parse(read('../../../../package.json')) as { dependencies?: Record<string, string>; devDependencies?: Record<string, string> };
    const names = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    expect(names.filter((n) => n.startsWith('@rollup/rollup-'))).toEqual([]);
    expect(names).not.toContain('@coral-xyz/anchor');
  });
});

describe('runs without a global Buffer (the browser has none)', () => {
  it('derives addresses, builds, signs and serializes a transaction without touching globalThis.Buffer', async () => {
    const { Keypair, PublicKey } = await import('@solana/web3.js');
    const { openPositionIx, assembleTransaction } = await import('./transactions');
    const { backerAddress, positionAddress, closeRecordAddress } = await import('./accounts');
    const { encodeInstructionData, decodeAccountData } = await import('./codecs');
    const { buildAccount } = await import('./testFixtures');

    const payer = Keypair.generate();
    const saved = (globalThis as { Buffer?: unknown }).Buffer;
    delete (globalThis as { Buffer?: unknown }).Buffer;
    try {
      // fresh seeds so the derivation cache cannot hide a global-Buffer dependency
      backerAddress(1, payer.publicKey);
      positionAddress(payer.publicKey, 12_345_678_901n);
      closeRecordAddress(987_654n);
      expect(encodeInstructionData('fund_operations', { amount: 5n })).toBeInstanceOf(Uint8Array);
      const ix = openPositionIx({
        owner: payer.publicKey,
        pool: decodeAccountData('Pool', buildAccount('Pool', { roundId: 3n })),
        plan: 'Simple',
        gross: 10_000_000n,
        nonce: 31_337n,
      });
      expect(ix.data).toBeInstanceOf(Uint8Array);
      const tx = assembleTransaction({
        feePayer: new PublicKey(payer.publicKey),
        blockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi',
        lastValidBlockHeight: 1,
        instructions: [ix],
        computeUnits: 200_000,
      });
      tx.partialSign(payer);
      expect(tx.serialize().length).toBeGreaterThan(100);
    } finally {
      (globalThis as { Buffer?: unknown }).Buffer = saved;
    }
  });
});
