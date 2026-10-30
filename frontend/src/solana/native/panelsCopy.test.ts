// IMPL-46c copy rules for the native panels: Front-End Load / Carried Interest / Jackpot Fee / AUM /
// Position / Backer / Series A,B / Emergency Equity Conversion; never casino or lottery words; and the
// stale 35/25/40 backer split is gone everywhere. Scans string literals and JSX text, not identifiers.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = fileURLToPath(new URL('.', import.meta.url));
const srcRoot = path.resolve(here, '../..');
const read = (relative: string) => readFileSync(path.join(srcRoot, relative), 'utf8');

/** User-facing text of a source file: string / template literals and JSX text, comments stripped. */
function userFacingText(source: string): string[] {
  const noBlock = source.replace(/\/\*[\s\S]*?\*\//g, ' ');
  const noLine = noBlock.replace(/(^|[^:'"`])\/\/[^\n]*/g, '$1');
  const out: string[] = [];
  for (const m of noLine.matchAll(/'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g)) out.push(m[1] ?? m[2] ?? m[3] ?? '');
  for (const m of noLine.matchAll(/>([^<>{}\n][^<>{}]*)</g)) out.push(m[1]);
  return out.filter((t) => /[A-Za-z]{3}/.test(t));
}

const BANNED = [/\bwinner/i, /\bwinning/i, /\blotter/i, /\blottery/i, /\bcasino/i, /\bjackpot(?! fee)/i, /\bdraw(s|n|ing)?\b/i, /\btickets?\b/i, /\bbet(s|ting)?\b/i];

// Files whose NEW native copy this card owns. Legacy strings that predate it (e.g. "Dice" icons, the
// responsible-gambling banner) are not banned words, so a whole-file scan is safe here.
const FILES = [
  'solana/native/panelsQuote.ts',
  'solana/native/panelsStatus.ts',
  'solana/native/panelsClosing.ts',
  'solana/native/panelsPrereqs.ts',
  'solana/native/panelsCluster.ts',
  'solana/native/panelsRail.ts',
  'components/SolInvestPanel.tsx',
  'components/SolBackerPanel.tsx',
  'components/GameTracking.tsx',
  'components/GameStatusBar.tsx',
  'components/WalletDropdown.tsx',
];

describe('no casino or lottery words in user-facing copy', () => {
  it('the scanner sees the copy it is meant to police (and would flag a banned word)', () => {
    const text = userFacingText(read('components/GameTracking.tsx'));
    expect(text.some((t) => /Allocation run in progress/.test(t))).toBe(true);
    expect(text.some((t) => /Confirm Withdrawal/.test(t))).toBe(true);
    expect(userFacingText(`const a = 'You are a winner'; const b = <p>Your ticket</p>;`).filter((t) => BANNED.some((re) => re.test(t)))).toHaveLength(2);
  });

  for (const file of FILES) {
    it(file, () => {
      const hits = userFacingText(read(file)).filter((text) => BANNED.some((re) => re.test(text)));
      expect(hits).toEqual([]);
    });
  }
});

describe('the backer split reads 35 / 45 / 20 everywhere', () => {
  it('HouseDashboard no longer hard-codes 25 % / 40 % and names the pro rata and equal pools', () => {
    const text = read('components/HouseDashboard.tsx');
    expect(text).not.toMatch(/25%/);
    expect(text).not.toMatch(/40%/);
    expect(text).toMatch(/BACKER_PROPORTIONAL_A_SHARE/);
    expect(text).toMatch(/pro rata across Series A Backers/);
    expect(text).toMatch(/equally across Series B Backers/);
  });

  it('DocsPage labels: 20 % is Series B only, 45 % is pro rata among Series A', () => {
    const text = read('components/DocsPage.tsx');
    expect(text).not.toMatch(/BACKER_OLDEST_UPSTREAM_SHARE|BACKER_OTHER_UPSTREAM_SHARE|BACKER_ALL_SHARE/);
    expect(text).toMatch(/pct\(BACKER_SENIOR_SHARE\)[^\n]*Oldest open Series A backer \(senior\)/);
    expect(text).toMatch(/pct\(BACKER_PROPORTIONAL_A_SHARE\)[^\n]*Series A backers, pro rata by remaining entitlement/);
    expect(text).toMatch(/pct\(BACKER_EQUAL_B_SHARE\)[^\n]*Series B backers, split equally/);
    expect(text).not.toMatch(/Split among all backers|Split among other Series A backers/);
  });

  it('the native Series A panel shows backerSplitText()', () => {
    expect(read('components/SolBackerPanel.tsx')).toMatch(/backerSplitText\(\)/);
  });
});

describe('browser findings F1-F3', () => {
  it('the invest, Series A and payout flows prepare shared pages first (F1)', () => {
    expect(read('components/SolInvestPanel.tsx')).toMatch(/runPrepared[\s\S]*openPrereqs/);
    expect(read('components/SolBackerPanel.tsx')).toMatch(/runPrepared[\s\S]*fundSeriesAPrereqs/);
    expect(read('components/GameTracking.tsx')).toMatch(/runPrepared[\s\S]*payoutPrereqs/);
  });

  it('the one-step "finalize and open" box is gone: it cannot provision round R+1\'s participant page (F2)', () => {
    const text = read('components/GameTracking.tsx');
    expect(text).not.toMatch(/ComposeOpenForm|composeExpireFinalizeOpen|Finalize and open/);
  });

  it('only finalize_round offers "Open a Position in the new round" (F3)', () => {
    const text = read('components/GameTracking.tsx');
    expect(text).toMatch(/outcome\.opensNewRound/);
    expect(text).toMatch(/processingOutcome\(sent\.kind\)/);
    // the call to action appears exactly once, behind that flag
    expect(text.match(/Open a Position in the new round/g)).toHaveLength(1);
  });
});

describe('the panels follow the rails', () => {
  it('SolInvestPanel projects on the gross deposit through projectSimpleDailyLamports', () => {
    const text = read('components/SolInvestPanel.tsx');
    expect(text).toMatch(/projectSimpleDailyLamports\(lamports\)/);
    expect(text).toMatch(/Front-End Load \(\{pct\(loadRate\)\}\)/);
  });

  it('GameTracking labels the legacy section and offers the Advanced contribution field', () => {
    const text = read('components/GameTracking.tsx');
    expect(text).toMatch(/Legacy \(Chain Fusion\) positions/);
    expect(text).toMatch(/Owner contribution to the round's processing costs, 0 by default/);
    expect(text).toMatch(/Allocation run in progress/);
    expect(text).toMatch(/resume processing/);
  });
});
