import { Principal } from '@dfinity/principal';
import { useQuery } from '@tanstack/react-query';
import { useReadShenaniganActor } from '../../hooks/useShenaniganActor';
import { useGetProfileFor } from '../../hooks/useQueries';

// Shared query factory so useDisplayName and useIsGolden hit the same React
// Query cache entry — calling both for the same principal makes one network
// round-trip, not two.
function useGoldenNameQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'goldenName', principalText],
    queryFn: async () => {
      if (!actor || !principal) return null;
      const result = await actor.getCustomDisplayName(principal);
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// Resolve a display name. Precedence: active golden-name spell > saved profile name > short principal slug.
/// Accepts null so callers can call the hook unconditionally; returns "" when null.
export function useDisplayName(principal: Principal | null): string {
  const principalText = principal?.toText() ?? '';
  const goldenQuery = useGoldenNameQuery(principal);
  const profileQuery = useGetProfileFor(principal ? principalText : undefined);

  if (goldenQuery.data) return goldenQuery.data;
  if (profileQuery.data?.name) return profileQuery.data.name;
  if (!principal) return '';
  return `${principalText.slice(0, 5)}…${principalText.slice(-3)}`;
}

/// True when the principal has an active Whitelisted (golden-name) spell.
/// Use to drive gold-coloring on the leaderboard or anywhere else that wants
/// to flag whitelisted players. Shares the cache with useDisplayName.
export function useIsGolden(principal: Principal | null): boolean {
  const goldenQuery = useGoldenNameQuery(principal);
  return Boolean(goldenQuery.data);
}
