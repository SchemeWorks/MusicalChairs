import { Principal } from '@dfinity/principal';
import { useQuery } from '@tanstack/react-query';
import { useReadShenaniganActor } from '../../hooks/useShenaniganActor';
import { useGetProfileFor } from '../../hooks/useQueries';

/// Resolve a display name. Precedence: active golden-name spell > saved profile name > short principal slug.
/// Accepts null so callers can call the hook unconditionally; returns "" when null.
export function useDisplayName(principal: Principal | null): string {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  const goldenQuery = useQuery({
    queryKey: ['shenanigans', 'goldenName', principalText],
    queryFn: async () => {
      if (!actor || !principal) return null;
      const result = await actor.getCustomDisplayName(principal);
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
  const profileQuery = useGetProfileFor(principal ? principalText : undefined);

  if (goldenQuery.data) return goldenQuery.data;
  if (profileQuery.data?.name) return profileQuery.data.name;
  if (!principal) return '';
  return `${principalText.slice(0, 5)}…${principalText.slice(-3)}`;
}
