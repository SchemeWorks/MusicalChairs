import React from 'react';
import type { ChatItem, ShenaniganRecord } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';
import { useGetShenaniganConfigs } from '../../../hooks/useQueries';

// Order matches backend shenanigan IDs (0-indexed). Used to map a variant tag
// like { moneyTrickster: null } back to the config id so we can look up the
// (admin-editable) spell name.
const SHEN_VARIANT_ORDER = [
  'moneyTrickster', 'aoeSkim', 'renameSpell', 'mintTaxSiphon', 'downlineHeist',
  'magicMirror', 'ppBoosterAura', 'purseCutter', 'whaleRebalance', 'downlineBoost', 'goldenName',
] as const;

const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

interface Props {
  item: ChatItem;
  spellLookup: Map<string, ShenaniganRecord>;
}

export default function SpellRow({ item, spellLookup }: Props) {
  const castId = 'spellCast' in item.kind ? item.kind.spellCast.castId : null;
  const record = castId !== null ? spellLookup.get(castId.toString()) : undefined;
  const userName = useDisplayName(record?.user ?? null);
  const target = record?.target?.[0] ?? null;
  const targetName = useDisplayName(target);
  const { data: configs = [] } = useGetShenaniganConfigs();

  if (!('spellCast' in item.kind)) return null;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  if (!record) {
    return (
      <div className="px-3 py-1 text-xs text-zinc-500">
        ✨ Someone cast a spell.
      </div>
    );
  }
  const outcomeText = 'success' in record.outcome ? 'landed clean' : 'backfire' in record.outcome ? 'backfired' : 'fizzled';
  const outcomeColor = 'success' in record.outcome ? 'text-emerald-300' : 'backfire' in record.outcome ? 'text-red-400' : 'text-zinc-400';
  const variantId = SHEN_VARIANT_ORDER.indexOf(variantKey(record.shenaniganType) as typeof SHEN_VARIANT_ORDER[number]);
  const spellName = configs.find(c => Number(c.id) === variantId)?.name ?? 'a spell';
  return (
    <div className="px-3 py-1 text-xs">
      <span className={`${outcomeColor} font-medium`}>✨ {userName}</span>
      <span className="text-zinc-400"> cast </span>
      <span className="text-zinc-200 font-medium">{spellName}</span>
      {target ? (
        <>
          <span className="text-zinc-400"> on </span>
          <span className="text-zinc-200 font-medium">{targetName}</span>
        </>
      ) : null}
      <span className="text-zinc-400"> — {outcomeText}.</span>
    </div>
  );
}
