import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';
import { useGetShenaniganConfigs } from '../../../hooks/useQueries';

// Order matches backend shenanigan IDs (0-indexed). Used to map a variant tag
// like { moneyTrickster: null } back to the config id so we can look up the
// (admin-editable) spell name.
const SHEN_VARIANT_ORDER = [
  'moneyTrickster', 'aoeSkim', 'renameSpell', 'mintTaxSiphon', 'downlineHeist',
  'magicMirror', 'ppBoosterAura', 'purseCutter', 'whaleRebalance', 'downlineBoost', 'goldenName',
  'tenderOffer', 'stimulusCheck', 'bearRaid',
] as const;

const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

interface Props {
  item: ChatItem;
}

// Spell-cast chat items embed caster/spell/target/outcome inline (see the
// #spellCast variant in shenanigans/main.mo). No lookup needed — render
// straight from the chat item. The spell *name* still comes from the
// admin-editable ShenaniganConfig so renames flow through retroactively.
function formatPp(units: bigint | number): string {
  const n = typeof units === 'bigint' ? Number(units) : units;
  const pp = n / 100_000_000;
  if (Number.isInteger(pp)) return pp.toLocaleString();
  return pp.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

export default function SpellRow({ item }: Props) {
  if (!('spellCast' in item.kind)) return null;
  const cast = item.kind.spellCast;
  const userName = useDisplayName(cast.caster);
  const target = cast.target?.[0] ?? null;
  const targetName = useDisplayName(target);
  const isCasterGolden = useIsGolden(cast.caster);
  const isTargetGolden = useIsGolden(target);
  const { data: configs = [] } = useGetShenaniganConfigs();

  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;

  const shieldDeflected = cast.shieldDeflected?.[0] ?? false;
  const ppDelta = cast.ppDelta?.[0] ?? null;
  const affected = cast.affectedCount?.[0] ?? null;
  const renameDetail = cast.renameDetail?.[0] ?? null;

  const outcomeText =
    shieldDeflected ? 'deflected' :
    'success' in cast.outcome ? 'landed clean' :
    'backfire' in cast.outcome ? 'backfired' : 'fizzled';
  const outcomeColor =
    shieldDeflected ? 'text-zinc-400' :
    'success' in cast.outcome ? 'text-emerald-300' :
    'backfire' in cast.outcome ? 'text-red-400' : 'text-zinc-400';
  const variantId = SHEN_VARIANT_ORDER.indexOf(variantKey(cast.shenaniganType) as typeof SHEN_VARIANT_ORDER[number]);
  const spellName = configs.find(c => Number(c.id) === variantId)?.name ?? 'a spell';

  let detailSuffix: React.ReactNode = null;
  if (renameDetail) {
    detailSuffix = (
      <span className="text-zinc-400">
        {' — '}
        <span className="text-zinc-200">{renameDetail.oldName}</span>
        {' → '}
        <span className="text-zinc-200">{renameDetail.newName}</span>
      </span>
    );
  } else if (ppDelta !== null && ppDelta !== 0n) {
    const ppNum = typeof ppDelta === 'bigint' ? ppDelta : BigInt(ppDelta);
    const sign = ppNum > 0n ? '+' : '';
    const acrossText = affected !== null && Number(affected) > 1
      ? ` across ${Number(affected)}`
      : '';
    detailSuffix = (
      <span className={`${ppNum > 0n ? 'text-emerald-300' : 'text-red-400'}`}>
        {' '}({sign}{formatPp(ppNum)} PP{acrossText})
      </span>
    );
  }

  return (
    <div className="px-3 py-1 text-xs">
      <span className={`${outcomeColor} font-medium`}>
        ✨{' '}
        {isCasterGolden ? (
          <GoldenName name={userName} isGolden={true} className="font-medium" />
        ) : (
          userName
        )}
      </span>
      <span className="text-zinc-400"> cast </span>
      <span className="text-zinc-200 font-medium">{spellName}</span>
      {target ? (
        <>
          <span className="text-zinc-400"> on </span>
          {isTargetGolden ? (
            <GoldenName name={targetName} isGolden={true} className="font-medium" />
          ) : (
            <span className="text-zinc-200 font-medium">{targetName}</span>
          )}
        </>
      ) : null}
      <span className="text-zinc-400"> — </span>
      <span className={outcomeColor}>{outcomeText}</span>
      <span className="text-zinc-400">.</span>
      {detailSuffix}
    </div>
  );
}
