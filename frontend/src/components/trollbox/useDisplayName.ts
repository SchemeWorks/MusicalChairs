import { Principal } from '@dfinity/principal';
import { useQuery } from '@tanstack/react-query';
import { useReadShenaniganActor } from '../../hooks/useShenaniganActor';
import { useGetProfileFor } from '../../hooks/useQueries';

// Active custom display name (set by renameSpell, id 2). Drives the rendered
// name in useDisplayName. NOT the source of truth for "is this player
// whitelisted" — see useGoldenStatusQuery for that.
function useCustomDisplayNameQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'customDisplayName', principalText],
    queryFn: async () => {
      if (!actor || !principal) return null;
      const result = await actor.getCustomDisplayName(principal);
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

// Live active-spell effects for a principal. We only consume `.golden` here
// (drives useIsGolden), but the full record is cached so other callers that
// want shield charges etc. for arbitrary principals can reuse it later.
function useGoldenStatusQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'goldenStatus', principalText],
    queryFn: async () => {
      if (!actor || !principal) return false;
      const effects = await actor.getActiveSpellEffects(principal);
      return Boolean(effects?.golden);
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// Resolve a display name. Precedence: custom name (renameSpell) > saved profile name > short principal slug.
/// Accepts null so callers can call the hook unconditionally; returns "" when null.
export function useDisplayName(principal: Principal | null): string {
  const principalText = principal?.toText() ?? '';
  const customNameQuery = useCustomDisplayNameQuery(principal);
  const profileQuery = useGetProfileFor(principal ? principalText : undefined);

  if (customNameQuery.data) return customNameQuery.data;
  if (profileQuery.data?.name) return profileQuery.data.name;
  if (!principal) return '';
  return `${principalText.slice(0, 5)}…${principalText.slice(-3)}`;
}

/// True when the principal has an active Whitelisted (goldenName, id 10) spell.
/// Use to drive gold-coloring on the leaderboard or anywhere else that wants
/// to flag whitelisted players. Backed by getActiveSpellEffects(principal).golden.
export function useIsGolden(principal: Principal | null): boolean {
  const goldenQuery = useGoldenStatusQuery(principal);
  return Boolean(goldenQuery.data);
}
