// IMPL-46c: bundle hygiene for the panels wiring. The main chunk must not pick up @solana/web3.js, and
// the local-cluster wallet must only ever be reached through a dynamic import guarded by LOCAL_CLUSTER_MODE.
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = fileURLToPath(new URL('.', import.meta.url));
const srcRoot = path.resolve(here, '../..');
const read = (file: string) => readFileSync(file, 'utf8');

/** Import statements that load a module at runtime (everything except `import type ...`). */
function staticValueImports(source: string): string[] {
  const statements = source.match(/^import\s[\s\S]*?from\s+['"][^'"]+['"];?/gm) ?? [];
  return statements.filter((s) => !/^import\s+type\s/.test(s)).map((s) => /from\s+['"]([^'"]+)['"]/.exec(s)![1]);
}

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === 'declarations' || entry.name === 'node_modules' ? [] : walk(full);
    return /\.(ts|tsx)$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) && !entry.name.endsWith('.d.ts') ? [full] : [];
  });
}

describe('panels wiring stays out of the main chunk', () => {
  const MAIN_CHUNK_SAFE = ['localModeFlag.ts', 'localModeLoader.ts', 'panelsQuote.ts', 'panelsStatus.ts', 'panelsCluster.ts', 'panelsClosing.ts', 'panelsPrereqs.ts', 'panelsRail.ts', 'panelsHooks.ts'];
  it.each(MAIN_CHUNK_SAFE)('%s value-imports neither @solana/* nor the web3-using native modules', (file) => {
    const imports = staticValueImports(read(path.join(here, file)));
    expect(imports.filter((i) => i.startsWith('@solana/')), file).toEqual([]);
    expect(imports.filter((i) => /\/(accounts|transactions|client|wallet|localModeWallet)$/.test(i)), file).toEqual([]);
  });

  it('the components load nothing from web3.js or the local wallet statically', () => {
    for (const name of ['SolInvestPanel', 'GameTracking', 'SolBackerPanel', 'AddBackerMoney', 'HouseDashboard', 'GameStatusBar', 'WalletDropdown', 'DocsPage']) {
      const imports = staticValueImports(read(path.join(srcRoot, 'components', `${name}.tsx`)));
      expect(imports.filter((i) => /localModeWallet$/.test(i)), name).toEqual([]);
      expect(imports.filter((i) => /solana\/native\/(accounts|transactions|client|wallet)$/.test(i)), name).toEqual([]);
    }
  });

  it('hooks/useWallet.tsx value-imports only the flag and the loader from the native modules', () => {
    const imports = staticValueImports(read(path.join(srcRoot, 'hooks', 'useWallet.tsx')));
    expect(imports.filter((i) => /solana\/native\//.test(i))).toEqual(['../solana/native/localModeFlag', '../solana/native/localModeLoader']);
  });
});

describe('the local-cluster wallet is only reached through localModeLoader', () => {
  const LOADS_LOCAL_WALLET = /import\(\s*['"][^'"]*localModeWallet['"]\s*\)/;

  it('exactly one dynamic import of localModeWallet exists, in localModeLoader.ts, as the consequent of a LOCAL_CLUSTER_MODE ternary', () => {
    const sites: string[] = [];
    for (const file of walk(srcRoot)) {
      read(file)
        .split('\n')
        .forEach((line, i) => {
          if (LOADS_LOCAL_WALLET.test(line) && !/^\s*(\/\/|\*)/.test(line) && !/typeof\s+import\(/.test(line)) sites.push(`${path.relative(srcRoot, file)}:${i + 1}`);
        });
    }
    expect(sites).toHaveLength(1);
    expect(sites[0].startsWith(path.join('solana', 'native', 'localModeLoader.ts'))).toBe(true);
    const loader = read(path.join(here, 'localModeLoader.ts'));
    expect(loader).toMatch(/LOCAL_CLUSTER_MODE\s*\?\s*\(\)\s*=>\s*import\('\.\/localModeWallet'\)\s*:\s*undefined/);
  });

  it('use sites go through loadLocalModeWallet, never a direct import', () => {
    for (const file of ['hooks/useWallet.tsx', 'solana/native/panelsHooks.ts']) {
      const text = read(path.join(srcRoot, file));
      expect(text, file).toMatch(/loadLocalModeWallet/);
      expect(text, file).not.toMatch(LOADS_LOCAL_WALLET);
    }
  });

  it('no other app module imports localModeWallet statically (types aside)', () => {
    const offenders = walk(srcRoot)
      .filter((file) => !file.endsWith(path.join('native', 'localModeWallet.ts')))
      .filter((file) => staticValueImports(read(file)).some((i) => /localModeWallet$/.test(i)));
    expect(offenders).toEqual([]);
  });

  it('UnsafeBurnerWalletAdapter is referenced only inside localModeWallet.ts', () => {
    const offenders = walk(srcRoot)
      .filter((file) => !file.endsWith(path.join('native', 'localModeWallet.ts')))
      .filter((file) => read(file).includes('UnsafeBurnerWalletAdapter'));
    expect(offenders).toEqual([]);
  });

  it('the dev chip in WalletDropdown and the connect function in useWallet are gated by the constant', () => {
    const dropdown = read(path.join(srcRoot, 'components', 'WalletDropdown.tsx'));
    expect(dropdown).toMatch(/LOCAL_CLUSTER_MODE && connectLocalCluster && walletType === 'none'/);
    const wallet = read(path.join(srcRoot, 'hooks', 'useWallet.tsx'));
    // A conditional expression (not a useCallback(...) call, which no bundler may drop) keeps the handler out of production.
    expect(wallet).toMatch(/const connectLocalCluster = LOCAL_CLUSTER_MODE\s*\?/);
    expect(wallet).toMatch(/const localHost = LOCAL_CLUSTER_MODE\s*\?/);
    expect(wallet).not.toMatch(/useCallback\(async \(opts/);
    expect(wallet).toMatch(/isLocalCluster: LOCAL_CLUSTER_MODE/);
  });
});

describe('local-cluster mode makes no ICP call', () => {
  it('useWallet.tsx hands the setters to localModeWallet and never touches the SIWS provider on that path', () => {
    const wallet = read(path.join(srcRoot, 'hooks', 'useWallet.tsx'));
    const local = wallet.slice(wallet.indexOf('const localHost'), wallet.indexOf('// Initialization - Check for existing sessions'));
    expect(local).toContain('adoptLocalSession');
    expect(local).not.toMatch(/siwsSigner|connectSiws|AuthClient|Actor|HttpAgent/);
    const init = wallet.slice(wallet.indexOf('if (LOCAL_CLUSTER_MODE && loadLocalModeWallet) {', wallet.indexOf('setAuthClient(client)')), wallet.indexOf('// Check for saved wallet type'));
    expect(init).toContain('restoreLocalClusterWallet');
    expect(init).toContain('return;');
    expect(init).not.toMatch(/siwsSigner|connectSiws|Actor|HttpAgent/);
  });

  it('localModeWallet.ts adopts the session with walletType siws and no identity', () => {
    const source = read(path.join(here, 'localModeWallet.ts'));
    const adopt = source.slice(source.indexOf('export function adoptLocalSession'), source.indexOf('/** Forget the local session'));
    expect(adopt).toContain('host.setIdentity(null)');
    expect(adopt).toContain("host.setWalletType('siws')");
  });

  it('localModeWallet.ts imports no canister, agent or SIWS code', () => {
    const imports = staticValueImports(read(path.join(here, 'localModeWallet.ts')));
    expect(imports.filter((i) => /siws|agent|auth-client|declarations|oisy/i.test(i))).toEqual([]);
  });
});

describe('the ICP bridge has exactly one frontend consumer (IMPL-49c)', () => {
  const ALLOWED = [path.join('hooks', 'useNativeHistory.ts')];
  const isBridgeUse = (text: string) =>
    /declarations\/native_sol_bridge/.test(text) || /\bgetRecentEvents\b/.test(text) || /VITE_NATIVE_BRIDGE_CANISTER_ID/.test(text);

  it('only hooks/useNativeHistory.ts imports the bridge declarations or calls its methods', () => {
    const offenders = walk(srcRoot)
      .filter((file) => !file.includes(`${path.sep}declarations${path.sep}`))
      .filter((file) => !ALLOWED.some((allowed) => file.endsWith(allowed)))
      .filter((file) => isBridgeUse(read(file)))
      .map((file) => path.relative(srcRoot, file));
    expect(offenders).toEqual([]);
    expect(isBridgeUse(read(path.join(srcRoot, 'hooks', 'useNativeHistory.ts')))).toBe(true);
  });

  it('the feed component reads the hook, never the actor, and the bridge never feeds a balance', () => {
    const feed = read(path.join(srcRoot, 'components', 'NativeHistoryFeed.tsx'));
    expect(feed).toMatch(/useNativeHistory/);
    expect(feed).not.toMatch(/Actor|HttpAgent|declarations/);
    // financial reads stay on Solana: nothing in the history hook is a balance, position or claim read
    const hook = read(path.join(srcRoot, 'hooks', 'useNativeHistory.ts')).replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
    expect(hook).not.toMatch(/getBalance|readPool|readPosition|listPositions|claimBacker/);
  });

  it('the hook queries anonymously (no identity, no update calls) and is disabled without a canister id', () => {
    const hook = read(path.join(srcRoot, 'hooks', 'useNativeHistory.ts')).replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
    expect(hook).not.toMatch(/identity|useWallet\(\)|signTransaction/);
    expect(hook).toMatch(/enabled: canisterId !== null/);
    expect(hook).toMatch(/getRecentEvents/);
    // it asks only for the verified-history method, never the operator/intake ones
    expect(hook).not.toMatch(/getPending|getUnbound|getEvent\b|getIntakeEvents|verifyAndForward|submitEventDescriptor/);
  });
});
