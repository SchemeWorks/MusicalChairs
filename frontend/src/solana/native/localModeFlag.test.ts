// IMPL-46c: local-cluster mode is dev-only. These tests pin (1) the gate, (2) that the
// production path is false whatever the env says, (3) that the mode really compiles out of
// a bundle built with the production define, and (4) the loopback-only guard.
import { describe, it, expect } from 'vitest';
import { fileURLToPath } from 'node:url';
import {
  LOCAL_CLUSTER_MODE,
  LOOPBACK_RPC_HOSTS,
  LoopbackGuardError,
  assertLoopbackRpcEndpoint,
  isLocalClusterMode,
  isLoopbackRpcEndpoint,
} from './localModeFlag';

describe('local-cluster mode gate', () => {
  it('needs BOTH the dev build and VITE_NATIVE_LOCAL_CLUSTER=1', () => {
    expect(isLocalClusterMode(true, '1')).toBe(true);
    expect(isLocalClusterMode(true, '0')).toBe(false);
    expect(isLocalClusterMode(true, 'true')).toBe(false);
    expect(isLocalClusterMode(true, '')).toBe(false);
    expect(isLocalClusterMode(true, undefined)).toBe(false);
  });

  it('is false in a production build whatever the flag says (import.meta.env.DEV is false there)', () => {
    for (const flag of ['1', '0', '', undefined, 'true']) {
      expect(isLocalClusterMode(false, flag)).toBe(false);
    }
  });

  it('the constant the app uses agrees with the pure gate for this test run', () => {
    // (With VITE_NATIVE_LOCAL_CLUSTER=1 in the repo-root .env.local, vitest is a DEV build and this is true.)
    expect(LOCAL_CLUSTER_MODE).toBe(isLocalClusterMode(import.meta.env.DEV, process.env.VITE_NATIVE_LOCAL_CLUSTER));
  });
});

describe('the production define compiles the mode out of a bundle', () => {
  const dir = fileURLToPath(new URL('.', import.meta.url));
  // The exact patterns the app uses, including the one that bites: a function handed to a hook-like call
  // (useCallback / useEffect), where Rollup includes a dynamic import it cannot prove unreachable even when
  // the `if` test folds to false. The module is therefore reached ONLY through loadLocalModeWallet
  // (localModeLoader.ts), a ternary on the folded flag.
  const entry = `
    import { LOCAL_CLUSTER_MODE } from './localModeFlag.ts';
    import { loadLocalModeWallet } from './localModeLoader.ts';
    declare const hook: (fn: () => unknown) => void;
    hook(async () => {
      if (LOCAL_CLUSTER_MODE && loadLocalModeWallet) {
        const mod = await loadLocalModeWallet();
        return mod.connectLocalClusterWallet();
      }
      return null;
    });
    export const connect = LOCAL_CLUSTER_MODE
      ? async () => (await loadLocalModeWallet!()).connectLocalClusterWallet()
      : undefined;
  `;
  // Rollup is Vite's production bundler: it folds `if (LOCAL_CLUSTER_MODE)` across the whole module graph
  // (esbuild cannot, it includes a dynamically imported module before constants are known), so the
  // check runs through the same tree-shaker the release build uses.
  async function bundle(dev: boolean, flag: string) {
    const [{ rollup }, { transform }, { readFileSync }, path] = await Promise.all([
      import('rollup'),
      import('esbuild'),
      import('node:fs'),
      import('node:path'),
    ]);
    const define = {
      'import.meta.env.DEV': String(dev),
      'process.env.VITE_NATIVE_LOCAL_CLUSTER': JSON.stringify(flag),
      'process.env.VITE_SOLANA_RPC_URL': JSON.stringify('http://127.0.0.1:8899'),
    };
    const entryId = path.join(dir, '__entry.ts');
    const build = await rollup({
      input: entryId,
      external: (id) => /^(@solana\/|@dfinity\/|bs58$)/.test(id),
      onwarn: () => undefined,
      plugins: [
        {
          name: 'test-ts',
          resolveId(id, importer) {
            if (id === entryId) return id;
            if (importer && (id.startsWith('./') || id.startsWith('../'))) {
              const resolved = path.resolve(path.dirname(importer), id);
              const file = resolved.endsWith('.ts') ? resolved : `${resolved}.ts`;
              // The web3-free helpers (IDL json and friends) are not what is being tested: keep them out of the graph.
              return /\/(codecs|queries|cluster)\.ts$/.test(file) ? { id: file, external: true } : file;
            }
            return null;
          },
          load(id) {
            if (id === entryId) return entry;
            return id.endsWith('.ts') ? readFileSync(id, 'utf8') : null;
          },
          async transform(code, id) {
            if (!id.endsWith('.ts')) return null;
            const out = await transform(code, { loader: 'ts', define, sourcefile: id });
            return { code: out.code, map: null };
          },
        },
      ],
    });
    const { output } = await build.generate({ format: 'es' });
    await build.close();
    return output.map((chunk) => ('code' in chunk ? chunk.code : '')).join('\n');
  }
  const LOCAL_ONLY_MARKERS = ['mc-native-local-cluster-secret-v1', 'Local cluster tester', 'connect a wallet'];

  it('production (DEV=false) with the flag set: no local-mode code or strings in any chunk', async () => {
    const out = await bundle(false, '1');
    for (const marker of LOCAL_ONLY_MARKERS) expect(out, marker).not.toContain(marker);
  });

  it('production without the flag: also gone', async () => {
    const out = await bundle(false, '');
    for (const marker of LOCAL_ONLY_MARKERS) expect(out, marker).not.toContain(marker);
  });

  it('dev without the flag: gone (the flag is required)', async () => {
    const out = await bundle(true, '');
    for (const marker of LOCAL_ONLY_MARKERS) expect(out, marker).not.toContain(marker);
  });

  it('control: dev WITH the flag keeps the local wallet code (so the checks above prove something)', async () => {
    const out = await bundle(true, '1');
    for (const marker of LOCAL_ONLY_MARKERS) expect(out, marker).toContain(marker);
  });
});

describe('vite.config exposes the flag to the browser through define', () => {
  it('defines process.env.VITE_NATIVE_LOCAL_CLUSTER from the environment, next to VITE_SOLANA_RPC_URL', async () => {
    // Loaded by path (not a typed import): vite.config.ts belongs to tsconfig.node.json, not the app project.
    const configPath = fileURLToPath(new URL('../../../../vite.config.ts', import.meta.url));
    const mod = (await import(/* @vite-ignore */ configPath)) as { default: unknown };
    const factory = mod.default as unknown as (env: { mode: string; command: string }) => { define: Record<string, string> };
    const before = process.env.VITE_NATIVE_LOCAL_CLUSTER;
    try {
      // process.env wins over the .env files, so these two cases are independent of the repo's .env.local.
      process.env.VITE_NATIVE_LOCAL_CLUSTER = '1';
      const on = factory({ mode: 'development', command: 'serve' }).define;
      expect(on['process.env.VITE_NATIVE_LOCAL_CLUSTER']).toBe('"1"');
      expect(Object.keys(on)).toContain('process.env.VITE_SOLANA_RPC_URL');
      process.env.VITE_NATIVE_LOCAL_CLUSTER = '0';
      const off = factory({ mode: 'production', command: 'build' }).define;
      expect(off['process.env.VITE_NATIVE_LOCAL_CLUSTER']).toBe('"0"');
      // Even a production build that carries the flag cannot turn the mode on: DEV is false there.
      expect(isLocalClusterMode(false, '1')).toBe(false);
    } finally {
      if (before === undefined) delete process.env.VITE_NATIVE_LOCAL_CLUSTER;
      else process.env.VITE_NATIVE_LOCAL_CLUSTER = before;
    }
  });
});

describe('loopback guard', () => {
  it('accepts exactly 127.0.0.1, localhost and [::1] over http(s)', () => {
    expect(LOOPBACK_RPC_HOSTS).toEqual(['127.0.0.1', 'localhost', '[::1]']);
    for (const ok of [
      'http://127.0.0.1:8899',
      'http://127.0.0.1',
      'http://localhost:8899',
      'https://localhost:8899/',
      'http://[::1]:8899',
      'http://LOCALHOST:8899',
      'http://user:pw@127.0.0.1:8899',
    ]) {
      expect(isLoopbackRpcEndpoint(ok), ok).toBe(true);
    }
  });

  it('refuses everything else, including look-alikes', () => {
    for (const bad of [
      'https://api.mainnet-beta.solana.com',
      'https://api.devnet.solana.com',
      'http://127.0.0.1.evil.com:8899',
      'http://localhost.evil.com',
      'http://evil.com/localhost',
      'http://localhost@evil.com',
      'http://0.0.0.0:8899',
      'http://127.0.0.2:8899',
      'http://192.168.1.10:8899',
      'http://10.0.0.5:8899',
      'http://foo.localhost:8899',
      'http://localhost.:8899',
      'ws://127.0.0.1:8900',
      'ftp://localhost',
      'localhost:8899',
      '127.0.0.1:8899',
      '',
      'not a url',
    ]) {
      expect(isLoopbackRpcEndpoint(bad), bad).toBe(false);
    }
  });

  it('assertLoopbackRpcEndpoint throws a LoopbackGuardError naming what it refused', () => {
    expect(() => assertLoopbackRpcEndpoint('http://127.0.0.1:8899', 'airdrop')).not.toThrow();
    expect(() => assertLoopbackRpcEndpoint('https://api.mainnet-beta.solana.com', 'airdrop')).toThrow(LoopbackGuardError);
    expect(() => assertLoopbackRpcEndpoint('https://api.mainnet-beta.solana.com', 'airdrop')).toThrow(/refuses to airdrop/);
  });
});
