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

// Strategic Reserve status query — calls getStrategicReserveStatus on the
// shenanigans canister. Returns true when the deadline is in the future.
function useStrategicReserveQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'strategicReserve', principalText],
    queryFn: async () => {
      if (!actor || !principal) return false;
      const result = await actor.getStrategicReserveStatus(principal);
      if (result.length === 0) return false;
      const deadline = Number(result[0]) / 1_000_000; // ns → ms
      return Date.now() < deadline;
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// True when the principal has an active Strategic Reserve (id 15) spell.
/// Use to drive purple-name rendering on the leaderboard.
export function useIsStrategicReserve(principal: Principal | null): boolean {
  const q = useStrategicReserveQuery(principal);
  return Boolean(q.data);
}

// Voice of God status query — calls getVoiceOfGodStatus on the shenanigans
// canister. Returns true when the deadline is in the future.
function useVoiceOfGodQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'voiceOfGodStatus', principalText],
    queryFn: async () => {
      if (!actor || !principal) return false;
      const result = await actor.getVoiceOfGodStatus(principal);
      if (result.length === 0) return false;
      const deadline = Number(result[0]) / 1_000_000; // ns → ms
      return Date.now() < deadline;
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// True when the principal has an active Voice of God (id 18) spell.
/// Use to drive the bolder chat-row styling.
export function useHasVoiceOfGod(principal: Principal | null): boolean {
  const q = useVoiceOfGodQuery(principal);
  return Boolean(q.data);
}

// Custom Title query — calls getCustomTitle on the shenanigans canister.
// Returns the title text while active, null/undefined otherwise.
function useCustomTitleQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'customTitle', principalText],
    queryFn: async () => {
      if (!actor || !principal) return null;
      const result = await actor.getCustomTitle(principal);
      return result.length === 0 ? null : result[0] ?? null;
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// Returns the active Custom Title (id 19) string for the principal, or null.
/// Use to render a ⟨Title⟩ suffix next to the display name in chat/leaderboard.
export function useCustomTitle(principal: Principal | null): string | null {
  const q = useCustomTitleQuery(principal);
  return q.data ?? null;
}

// Confetti Cannon status query — returns true when the deadline is in the future.
function useConfettiCannonQuery(principal: Principal | null) {
  const principalText = principal?.toText() ?? '';
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'confettiCannonStatus', principalText],
    queryFn: async () => {
      if (!actor || !principal) return false;
      const result = await actor.getConfettiCannonStatus(principal);
      if (result.length === 0) return false;
      const deadline = Number(result[0]) / 1_000_000; // ns → ms
      return Date.now() < deadline;
    },
    refetchInterval: 30000,
    enabled: !!actor && !!principal,
  });
}

/// True when the principal has an active Confetti Cannon (id 21) spell.
/// Use to trigger the confetti burst animation in the live feed.
export function useHasConfettiCannon(principal: Principal | null): boolean {
  const q = useConfettiCannonQuery(principal);
  return Boolean(q.data);
}
