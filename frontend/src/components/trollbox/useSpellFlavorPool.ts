import { useListFlavorPools } from '../../hooks/useQueries';
import { SPELL_FLAVOR_DEFAULTS, type SpellFlavorKey } from './spellFlavorDefaults';

/// Returns the effective pool for the given key, respecting any admin override.
/// Empty override (admin explicitly cleared the lines) is treated as "use defaults"
/// for the spell flavor case — we don't want spells to render blank.
export function useSpellFlavorPool(key: SpellFlavorKey): string[] {
  const { data: pools = [] } = useListFlavorPools();
  const entry = pools.find(([n]) => n === key);
  if (entry && entry[1].length > 0) return entry[1];
  return [...SPELL_FLAVOR_DEFAULTS[key]];
}
