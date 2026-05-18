import { Principal } from '@dfinity/principal';
import { useQuery } from '@tanstack/react-query';
import { useReadShenaniganActor } from '../../hooks/useShenaniganActor';

/// Resolve a display name with golden-name override taking precedence over
/// the user's saved profile name. Falls back to a short principal slug.
export function useDisplayName(principal: Principal): string {
  const principalText = principal.toText();
  const actor = useReadShenaniganActor();
  const goldenQuery = useQuery({
    queryKey: ['shenanigans', 'goldenName', principalText],
    queryFn: async () => {
      if (!actor) return null;
      const result = await actor.getCustomDisplayName(principal);
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: 30000,
    enabled: !!actor,
  });
  if (goldenQuery.data) return goldenQuery.data;
  return `${principalText.slice(0, 5)}…${principalText.slice(-3)}`;
}
