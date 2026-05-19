import { useEffect, useRef } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useActor } from './useActor';
import { useReadActor } from './useReadActor';
import { useShenaniganActor, useReadShenaniganActor } from './useShenaniganActor';
import { usePonziMathActor } from './usePonziMathActor';
import { useReadPonziMath } from './useReadPonziMath';
import { useWallet } from './useWallet';
import { useLedger, BACKEND_CANISTER_ID, ICP_LEDGER_CANISTER_ID, icrcLedgerIDL } from './useLedger';
import { useReadPpLedger, useAuthPpLedger, shenanigansOwner, principalToChipSubaccount, ppUnitsToWhole, wholePpToUnits } from './usePpLedger';
import { getOisySignerAgent, createOisyActor } from '../lib/oisySigner';
import { UserProfile, GameRecord, GamePlan, PlatformStats, ShenaniganType, ShenaniganOutcome, ShenaniganStats, ShenaniganRecord, BackerPosition, BackerKey, GeneralLedgerEntry, ActivePlanSnapshot, RoundSummary, ShenaniganConfig, ponziMathIdlFactory } from '../backend';
import { Principal } from '@dfinity/principal';
import { Actor, HttpAgent } from '@dfinity/agent';
import type { ChatItem } from '../declarations/shenanigans/shenanigans.did';
import { TROLLBOX_POLL_MS, TROLLBOX_PIN_POLL_MS, TROLLBOX_MUTE_POLL_MS, TROLLBOX_FETCH_LIMIT } from '../components/trollbox/trollboxConstants';
import { buildReferralLink, getStoredReferrer } from '../lib/referral';
import { isCharles } from '../lib/charles';
import {
  EXIT_TOLL_EARLY,
  EXIT_TOLL_MID,
  EXIT_TOLL_LATE,
  EXIT_TOLL_EARLY_DAYS,
  EXIT_TOLL_MID_DAYS,
  JACKPOT_FEE_RATE_15D,
  JACKPOT_FEE_RATE_30D,
} from '../lib/gameConstants';

// ponzi_math canister ID (matches the constant in usePonziMathActor.ts)
const PONZI_MATH_CANISTER_ID = 'guy42-yqaaa-aaaaj-qr5pq-cai';

// Anonymous ledger actor for balance queries. icrc1_balance_of is a public
// query — no identity needed. Cached at module scope so all callers share
// one actor regardless of wallet type.
let anonLedgerActor: any = null;
function getAnonLedgerActor() {
  if (anonLedgerActor) return anonLedgerActor;
  const agent = new HttpAgent({ host: 'https://icp0.io' });
  anonLedgerActor = Actor.createActor(icrcLedgerIDL, {
    agent,
    canisterId: ICP_LEDGER_CANISTER_ID,
  });
  return anonLedgerActor;
}

// User Profile Queries
export function useGetCallerUserProfile() {
  const actor = useReadActor();
  const { principal } = useWallet();

  const query = useQuery<UserProfile | null>({
    queryKey: ['currentUserProfile', principal],
    queryFn: async (): Promise<UserProfile | null> => {
      if (!principal) throw new Error('No principal');
      const result = await actor.getUserProfile(Principal.fromText(principal));
      // Convert Candid optional ([] | [UserProfile]) to UserProfile | null
      return result[0] ?? null;
    },
    enabled: !!principal,
    retry: false,
  });

  return {
    ...query,
    isLoading: query.isLoading,
    isFetched: query.isFetched,
  };
}

export function useGetUserNames(principals: string[]) {
  const actor = useReadActor();
  const key = [...new Set(principals)].sort().join(',');

  return useQuery<Map<string, string>>({
    queryKey: ['userNames', key],
    queryFn: async () => {
      const unique = [...new Set(principals)];
      const entries = await Promise.all(
        unique.map(async (p) => {
          try {
            const result = await actor.getUserProfile(Principal.fromText(p));
            const name = result[0]?.name?.trim();
            return [p, name && name.length > 0 ? name : ''] as const;
          } catch {
            return [p, ''] as const;
          }
        }),
      );
      return new Map(entries);
    },
    enabled: principals.length > 0,
    staleTime: 60_000,
  });
}

export function useSaveUserProfile() {
  const { actor } = useActor();

  return useMutation({
    mutationFn: async (profile: UserProfile) => {
      if (!actor) throw new Error('Actor not available');
      return actor.saveCallerUserProfile(profile);
    },
    // No onSuccess invalidation — ProfileSetup handles the delayed
    // invalidation after 5s so the celebration screen has time to
    // display before App.tsx swaps to Dashboard.
  });
}

// Game Statistics Queries
export function useGetGameStats() {
  const actor = useReadPonziMath();

  return useQuery<PlatformStats>({
    queryKey: ['gameStats'],
    queryFn: async () => actor.getPlatformStats(),
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
  });
}

// Public stats — no auth required, uses read actor for splash page
export function useGetPublicStats() {
  const actor = useReadPonziMath();
  return useQuery<PlatformStats>({
    queryKey: ['publicStats'],
    queryFn: async () => actor.getPlatformStats(),
    refetchInterval: 30000,
    staleTime: 15000,
  });
}

// Deposit Limits and Rate Limiting Queries
export function useGetMaxDepositLimit() {
  const actor = useReadPonziMath();

  return useQuery<number>({
    queryKey: ['maxDepositLimit'],
    queryFn: async () => actor.getMaxDepositLimit(),
    refetchInterval: 5000, // Refetch every 5 seconds as pot balance changes
  });
}

export function useCheckDepositRateLimit() {
  const { actor, isFetching: actorFetching } = usePonziMathActor();
  const { walletType } = useWallet();

  return useQuery<boolean>({
    queryKey: ['checkDepositRateLimit', walletType],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.checkDepositRateLimit();
    },
    enabled: !!actor && !actorFetching && walletType !== 'oisy',
    refetchInterval: 60000, // Refetch every minute to update rate limit status
  });
}

// Backer Repayment Balance Query
export function useGetBackerRepaymentBalance() {
  const actor = useReadPonziMath();
  const { principal } = useWallet();

  return useQuery<number>({
    queryKey: ['backerRepaymentBalance', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      return actor.getBackerRepaymentBalanceFor(Principal.fromText(principal));
    },
    enabled: !!principal,
    refetchInterval: 5000,
  });
}
// Legacy alias
export const useGetHouseRepaymentBalance = useGetBackerRepaymentBalance;

// All Backer Repayment Balances (public — matches backer roster visibility)
export function useGetAllBackerRepayments() {
  const actor = useReadPonziMath();

  return useQuery<Array<[BackerKey, number]>>({
    queryKey: ['allBackerRepayments'],
    queryFn: async () => actor.getAllBackerRepayments(),
    refetchInterval: 5000,
  });
}

// General Ledger Queries (renamed from House Ledger)
export function useGetGeneralLedger() {
  const actor = useReadPonziMath();

  return useQuery<GeneralLedgerEntry[]>({
    queryKey: ['generalLedger'],
    queryFn: async () => {
      try {
        const records = await actor.getGeneralLedger();
        return records || [];
      } catch (error: any) {
        console.error('Failed to fetch general ledger:', error);
        return [];
      }
    },
    refetchInterval: 5000,
    retry: 2,
    retryDelay: 1000,
    placeholderData: [],
  });
}
// Legacy alias for existing consumers
export const useGetHouseLedger = useGetGeneralLedger;

export function useGetGeneralLedgerStats() {
  const actor = useReadPonziMath();

  return useQuery({
    queryKey: ['generalLedgerStats'],
    queryFn: async () => {
      try {
        const stats = await actor.getGeneralLedgerStats();
        return stats || {
          totalInflows: 0,
          totalOutflows: 0,
          netFlow: 0,
          entryCount: BigInt(0)
        };
      } catch (error: any) {
        console.error('Failed to fetch general ledger stats:', error);
        return {
          totalInflows: 0,
          totalOutflows: 0,
          netFlow: 0,
          entryCount: BigInt(0)
        };
      }
    },
    refetchInterval: 5000,
    retry: 2,
    retryDelay: 1000,
    placeholderData: {
      totalInflows: 0,
      totalOutflows: 0,
      netFlow: 0,
      entryCount: BigInt(0)
    },
  });
}
// Legacy alias for existing consumers
export const useGetHouseLedgerStats = useGetGeneralLedgerStats;

// Real ICP balance from the NNS ledger (what the user actually has in their wallet).
// Uses an anonymous ledger actor — icrc1_balance_of is a public query and works
// for every wallet type (Plug, II, Oisy) without needing a signing identity.
export function useICPBalance() {
  const { principal } = useWallet();

  return useQuery({
    queryKey: ['icpLedgerBalance', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      const actor = getAnonLedgerActor();
      const balanceE8s = await actor.icrc1_balance_of({
        owner: Principal.fromText(principal),
        subaccount: [],
      });
      return Number(balanceE8s) / 100_000_000;
    },
    enabled: !!principal,
    refetchInterval: 5000,
  });
}

// ============================================================================
// Cover Charge — admin-only (Pay Management)
// ============================================================================

// Hardcoded — matches the COVER_CHARGE_RECIPIENT constant in backend/main.mo.
// Only this principal can successfully call getCoverChargeBalance and
// withdrawCoverCharges; the backend enforces this independently.
export const COVER_CHARGE_RECIPIENT =
  'gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe';

export function isCoverChargeAdmin(principal: string | null | undefined): boolean {
  return principal === COVER_CHARGE_RECIPIENT;
}

// Query the cover-charge bucket balance on ponzi_math.
// Now public — no admin gate needed on the frontend (ponzi_math enforces nothing here).
export function useGetCoverChargeBalance() {
  const actor = useReadPonziMath();
  const { principal } = useWallet();

  return useQuery({
    queryKey: ['coverChargeBalance', principal],
    queryFn: async () => {
      const balanceE8s = await actor.getCoverChargeBalance();
      return {
        e8s: balanceE8s,
        icp: Number(balanceE8s) / 100_000_000,
      };
    },
    // Still only show to admin in the UI; the query itself is public on ponzi_math
    enabled: !!principal && isCoverChargeAdmin(principal),
    refetchInterval: 5000,
  });
}

// Pay Management — admin calls backend.payManagement(to, amountE8s) which
// sweeps cover charges from ponzi_math and transfers `amountE8s` e8s to `to`.
export function usePayManagement() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ to, amountE8s }: { to: Principal; amountE8s: bigint }) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');

      const result = await actor.payManagement(to, amountE8s);

      if ('Err' in result) {
        throw new Error(result.Err);
      }

      return {
        success: true,
        blockIndex: result.Ok,
        amount: Number(amountE8s) / 100_000_000,
        timestamp: new Date(),
      };
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['coverChargeBalance'] });
      queryClient.refetchQueries({ queryKey: ['coverChargeBalance'] });
    },
  });
}
// Legacy alias — kept so WalletDropdown can be updated in Task 37
export const useWithdrawCoverCharges = usePayManagement;

// Add Backer Money Mutation — regular users become Series A backers
export function useAddBackerMoney() {
  const { actor } = usePonziMathActor();
  const { actor: shenActor } = useShenaniganActor();
  const shenReadActor = useReadShenaniganActor();
  const { principal, walletType } = useWallet();
  const queryClient = useQueryClient();
  const ledger = useLedger();

  return useMutation({
    mutationFn: async (amount: number) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');

      if (amount <= 0) throw new Error('Please enter a valid amount greater than 0.');

      const amountE8s = BigInt(Math.round(amount * 100_000_000));
      const approveAmount = amountE8s + 20_000n; // extra for fees

      // Register the referral chain on shenanigans BEFORE depositing so the
      // observer's cascade finds the chain set when it processes the new
      // backer entry. See ensureReferralRegistered for context.
      if (shenActor) {
        await ensureReferralRegistered(shenActor, shenReadActor, principal);
      }

      try {
        if (walletType === 'oisy') {
          // Oisy path: sequential approve + addBackerMoney (two popups).
          // batch()/execute() relied on ICRC-112, which no wallet adopted; a recent
          // Oisy update broke slide-computer's multi-ICRC-49 fallback, surfacing as
          // "Cannot read properties of undefined (reading '_arr')".
          const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
          const ledgerActor = createOisyActor(ICP_LEDGER_CANISTER_ID, icrcLedgerIDL, signerAgent);
          const ponziMathActor = createOisyActor(PONZI_MATH_CANISTER_ID, ponziMathIdlFactory, signerAgent);

          await ledgerActor.icrc2_approve({
            amount: approveAmount,
            spender: { owner: Principal.fromText(PONZI_MATH_CANISTER_ID), subaccount: [] },
            expires_at: [],
            expected_allowance: [],
            memo: [],
            fee: [],
            from_subaccount: [],
            created_at_time: [],
          });

          const addResult = await ponziMathActor.addBackerMoney(amount);
          if ('Err' in addResult) throw new Error(addResult.Err);
        } else {
          // Standard path: approve then addBackerMoney
          await ledger.approve(PONZI_MATH_CANISTER_ID, approveAmount);
          const addResult = await actor.addBackerMoney(amount);
          if ('Err' in addResult) throw new Error(addResult.Err);
        }

        return {
          success: true,
          amount,
          expectedReturn: amount * 1.24,
          timestamp: new Date()
        };
      } catch (error: any) {
        console.error('Failed to add backer money:', error);
        // Provide more user-friendly error messages
        if (error.message?.includes('Amount must be greater than 0')) {
          throw new Error('Please enter a valid amount greater than 0.');
        } else if (error.message?.includes('Minimum deposit is 0.1 ICP')) {
          throw new Error('Minimum deposit is 0.1 ICP.');
        } else if (error.message?.includes('decimal places')) {
          throw new Error('Amount cannot have more than 8 decimal places.');
        } else {
          throw new Error('Failed to deposit backer money. Please try again or contact support.');
        }
      }
    },
    onSuccess: () => {
      // Immediately invalidate and refetch all related queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.invalidateQueries({ queryKey: ['backerPositions'] });
      queryClient.invalidateQueries({ queryKey: ['generalLedger'] });
      queryClient.invalidateQueries({ queryKey: ['generalLedgerStats'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.refetchQueries({ queryKey: ['backerPositions'] });
      queryClient.refetchQueries({ queryKey: ['generalLedger'] });
      queryClient.refetchQueries({ queryKey: ['generalLedgerStats'] });
    },
  });
}
// Legacy alias
export const useAddDealerMoney = useAddBackerMoney;

// Store for tracking accumulated earnings per game
const gameEarningsStore = new Map<string, { lastUpdateTime: number; accumulatedEarnings: number }>();

// Game Queries with manual refresh functionality
export function useGetUserGames() {
  const actor = useReadPonziMath();
  const { principal } = useWallet();

  return useQuery<GameRecord[]>({
    queryKey: ['userGames', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      const allGames = await actor.getUserGamesFor(Principal.fromText(principal));
      // Hide closed positions (fully withdrawn compounding, matured simple)
      const games = allGames.filter(g => g.isActive);

      // Update accumulated earnings for each game when manually refreshed
      const currentTime = Date.now();
      games.forEach(game => {
        const gameKey = game.id.toString();
        const stored = gameEarningsStore.get(gameKey);
        
        if (!stored) {
          // First time seeing this game, initialize with current backend earnings
          gameEarningsStore.set(gameKey, {
            lastUpdateTime: currentTime,
            accumulatedEarnings: game.accumulatedEarnings
          });
        } else {
          // Calculate new earnings since last update and add to accumulated total
          const timeSinceLastUpdate = (currentTime - stored.lastUpdateTime) / 1000; // Convert to seconds
          const newEarnings = calculateIncrementalEarnings(game, timeSinceLastUpdate);
          
          gameEarningsStore.set(gameKey, {
            lastUpdateTime: currentTime,
            accumulatedEarnings: stored.accumulatedEarnings + newEarnings
          });
        }
      });
      
      return games;
    },
    enabled: !!principal,
    // Remove automatic refetch interval - now manual only
  });
}

export function useCreateGame() {
  const { actor } = usePonziMathActor();
  const { actor: shenActor } = useShenaniganActor();
  const shenReadActor = useReadShenaniganActor();
  const { principal, walletType } = useWallet();
  const queryClient = useQueryClient();
  const ledger = useLedger();

  return useMutation({
    mutationFn: async ({ planId, amount, mode }: { planId: string; amount: number; mode: 'simple' | 'compounding' }) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');

      let plan: GamePlan;
      switch (planId) {
        case '21-day-simple':
          plan = GamePlan.simple21Day;
          break;
        case '15-day-compounding':
          plan = GamePlan.compounding15Day;
          break;
        case '30-day-compounding':
          plan = GamePlan.compounding30Day;
          break;
        default:
          throw new Error('Invalid plan ID');
      }

      const amountE8s = BigInt(Math.round(amount * 100_000_000));
      const approveAmount = amountE8s + 20_000n; // extra for fees
      const isCompounding = mode === 'compounding';

      // Synchronously register the referral chain on shenanigans BEFORE
      // creating the game. The shenanigans observer reads referralChain at
      // mint time; if the chain isn't set yet when it sees the new game,
      // the 500 PP signup-gift cascade goes to "house" instead of up to
      // the upline. Awaiting here closes the race window. Idempotent and
      // skips with a query if already registered, so it's safe and cheap.
      if (shenActor) {
        await ensureReferralRegistered(shenActor, shenReadActor, principal);
      }

      let gameId: bigint;

      // Oisy path: sequential approve + createGame (two popups).
      // See useAddBackerMoney for the full reason batch()/execute() was removed.
      if (walletType === 'oisy') {
        const signerAgent = await getOisySignerAgent(Principal.fromText(principal));
        const ledgerActor = createOisyActor(ICP_LEDGER_CANISTER_ID, icrcLedgerIDL, signerAgent);
        const ponziMathActor = createOisyActor(PONZI_MATH_CANISTER_ID, ponziMathIdlFactory, signerAgent);

        await ledgerActor.icrc2_approve({
          amount: approveAmount,
          spender: { owner: Principal.fromText(PONZI_MATH_CANISTER_ID), subaccount: [] },
          expires_at: [],
          expected_allowance: [],
          memo: [],
          fee: [],
          from_subaccount: [],
          created_at_time: [],
        });

        const gameResult = await ponziMathActor.createGame(plan, amount, isCompounding);
        if ('Err' in gameResult) throw new Error(gameResult.Err);
        gameId = gameResult.Ok;
      } else {
        // Standard path: separate approve + createGame (3 args — no referrer)
        await ledger.approve(PONZI_MATH_CANISTER_ID, approveAmount);
        const gameResult = await actor.createGame(plan, amount, isCompounding);
        if ('Err' in gameResult) throw new Error(gameResult.Err);
        gameId = gameResult.Ok;
      }

      // Initialize earnings tracking for the new game
      gameEarningsStore.set(gameId.toString(), {
        lastUpdateTime: Date.now(),
        accumulatedEarnings: 0
      });

      // Calculate House Maintenance fee (3%)
      const houseFee = amount * 0.03;
      const netAmount = amount - houseFee;

      return {
        success: true,
        gameId,
        planId,
        amount,
        mode,
        houseFee,
        netAmount,
        timestamp: new Date()
      };
    },
    onSuccess: () => {
      // Immediately invalidate and refetch all related queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['userGames'] });
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.invalidateQueries({ queryKey: ['maxDepositLimit'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
    },
  });
}

export function useWithdrawGameEarnings() {
  const { actor } = usePonziMathActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (gameId: bigint) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      const result = await actor.withdrawEarnings(gameId);
      if ('Err' in result) throw new Error(result.Err);

      // Reset accumulated earnings for this game after withdrawal
      gameEarningsStore.delete(gameId.toString());

      return { earnings: result.Ok, gameId };
    },
    onSuccess: () => {
      // Immediately invalidate and refetch all related queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['userGames'] });
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.invalidateQueries({ queryKey: ['maxDepositLimit'] });
    },
  });
}

export function useSettleCompoundingGame() {
  const { actor } = usePonziMathActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (gameId: bigint) => {
      if (!actor) throw new Error('Actor not available');
      const result = await actor.settleCompoundingGame(gameId);
      if ('Err' in result) throw new Error(result.Err);
      gameEarningsStore.delete(gameId.toString());
      return { earnings: result.Ok, gameId };
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['userGames'] });
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
    },
  });
}

export function useClaimBackerRepayment() {
  const { actor } = usePonziMathActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!actor) throw new Error('Actor not available');
      const result = await actor.claimBackerRepayment();
      if ('Err' in result) throw new Error(result.Err);
      return result.Ok;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['backerPositions'] });
      queryClient.invalidateQueries({ queryKey: ['seedRoundDashboard'] });
    },
  });
}

export function useCalculateGameEarnings() {
  const { actor } = usePonziMathActor();

  return useMutation({
    mutationFn: async (game: GameRecord) => {
      if (!actor) throw new Error('Actor not available');
      return actor.calculateEarnings(game);
    },
  });
}

// Shenanigans Queries — routed to standalone shenanigans canister.
// Read paths use the anonymous read actor so that Oisy users don't get
// their query refetches upgraded to icrc49 update calls via the signer.
export function useGetShenaniganStats() {
  const actor = useReadShenaniganActor();
  const { principal } = useWallet();

  return useQuery<ShenaniganStats>({
    queryKey: ['shenaniganStats', principal],
    queryFn: async () => actor.getShenaniganStats(),
    refetchInterval: 5000,
  });
}

export function useGetRecentShenanigans() {
  const actor = useReadShenaniganActor();

  return useQuery<ShenaniganRecord[]>({
    queryKey: ['recentShenanigans'],
    queryFn: async () => actor.getRecentShenanigans(),
    refetchInterval: 3000,
  });
}

export function useGetKnownPpHolders() {
  const actor = useReadShenaniganActor();

  return useQuery<Principal[]>({
    queryKey: ['knownPpHolders'],
    queryFn: async () => actor.getKnownPpHolders(),
    staleTime: 30_000,
  });
}

export function useCastShenanigan() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ shenaniganType, target }: { shenaniganType: ShenaniganType; target: Principal | null }) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.castShenanigan(shenaniganType, target ? [target] : []);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganStats'] });
      queryClient.invalidateQueries({ queryKey: ['recentShenanigans'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
    },
  });
}

// Shenanigans Configuration Queries
export function useGetShenaniganConfigs() {
  const actor = useReadShenaniganActor();

  return useQuery<ShenaniganConfig[]>({
    queryKey: ['shenaniganConfigs'],
    queryFn: async () => actor.getShenaniganConfigs(),
    refetchInterval: 10000,
  });
}

export function useUpdateShenaniganConfig() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (config: ShenaniganConfig) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.updateShenaniganConfig(config);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
      queryClient.refetchQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useSaveAllShenaniganConfigs() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (configs: ShenaniganConfig[]) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.saveAllShenaniganConfigs(configs);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
      queryClient.refetchQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useResetShenaniganConfig() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: bigint) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      return actor.resetShenaniganConfig(id);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
      queryClient.refetchQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

// ROI Calculation Functions
export function calculateSimpleROI(amount: number, plan: string, days: number) {
  // Updated for Simple Mode: principal is consumed, only interest is paid out
  const dailyRate = getDailyRate(plan);
  const totalInterest = amount * dailyRate * days; // Interest only, no principal return
  const roiPercent = (totalInterest / amount) * 100;
  
  return {
    totalReturn: totalInterest, // Only interest is returned
    profit: totalInterest, // All of the payout is profit since principal is consumed
    roiPercent
  };
}

export function calculateCompoundingROI(amount: number, plan: string, days: number) {
  // Updated for Compounding Mode: principal is consumed, only compounded interest is paid out
  // Using the correct formula: payout = principal × [(1 + daily rate)^days - 1]
  const dailyRate = getDailyRate(plan);
  const compoundedInterest = amount * (Math.pow(1 + dailyRate, days) - 1); // Only compounded interest
  const roiPercent = (compoundedInterest / amount) * 100;
  
  return {
    totalReturn: compoundedInterest, // Only compounded interest is returned
    profit: compoundedInterest, // All of the payout is profit since principal is consumed
    roiPercent
  };
}

export function getDailyRate(plan: string): number {
  switch (plan) {
    case '21-day-simple':
      return 0.11; // 11% daily for simple mode
    case '15-day-compounding':
      return 0.12; // 12% daily for 15-day compounding
    case '30-day-compounding':
      return 0.09; // 9% daily for 30-day compounding
    default:
      return 0.11;
  }
}

export function getPlanDays(plan: string): number {
  switch (plan) {
    case '21-day-simple':
      return 21;
    case '15-day-compounding':
      return 15;
    case '30-day-compounding':
      return 30;
    default:
      return 21;
  }
}

// Helper function to calculate incremental earnings for a time period (with plan duration cap)
function calculateIncrementalEarnings(game: GameRecord, elapsedSeconds: number): number {
  const dailyRate = getDailyRate(getGamePlanString(game.plan));
  const planDays = getPlanDays(getGamePlanString(game.plan));
  const maxDurationSeconds = planDays * 86400;

  if (game.isCompounding) {
    // Cap elapsed time at plan duration
    const cappedSeconds = Math.min(elapsedSeconds, maxDurationSeconds);
    const elapsedDays = cappedSeconds / 86400;
    const compoundedInterest = game.amount * (Math.pow(1 + dailyRate, elapsedDays) - 1);

    // Return incremental earnings since last update
    const stored = gameEarningsStore.get(game.id.toString());
    const previousEarnings = stored ? stored.accumulatedEarnings : game.accumulatedEarnings;

    return Math.max(0, compoundedInterest - previousEarnings);
  } else {
    // Simple: cap at remaining allowed time (accounts for previous claims)
    const startNs = Number(game.startTime);
    const lastUpdateNs = Number(game.lastUpdateTime);
    const timeAlreadyAccounted = (lastUpdateNs - startNs) / 1_000_000_000;
    const remainingAllowed = Math.max(0, maxDurationSeconds - timeAlreadyAccounted);
    const cappedSeconds = Math.min(elapsedSeconds, remainingAllowed);

    return game.amount * dailyRate * (cappedSeconds / 86400);
  }
}

// Helper function to calculate current total accumulated earnings from stored values
export function calculateCurrentEarnings(game: GameRecord): number {
  const stored = gameEarningsStore.get(game.id.toString());
  if (!stored) {
    // If not in store yet, return backend accumulated earnings
    return game.accumulatedEarnings;
  }
  
  // Return the accumulated total from our store
  return stored.accumulatedEarnings;
}

// Helper function to check if compounding plan is unlocked
export function isCompoundingPlanUnlocked(game: GameRecord): boolean {
  if (!game.isCompounding) return true; // Simple mode is always unlocked
  
  const planDays = getPlanDays(getGamePlanString(game.plan));
  const startTime = Number(game.startTime) / 1000000; // Convert nanoseconds to milliseconds
  const elapsedTime = Date.now() - startTime;
  const elapsedDays = elapsedTime / (1000 * 60 * 60 * 24);
  
  return elapsedDays >= planDays;
}

// Helper function to get time remaining for compounding plans
export function getTimeRemaining(game: GameRecord): { days: number; hours: number; minutes: number } {
  if (!game.isCompounding) return { days: 0, hours: 0, minutes: 0 };
  
  const planDays = getPlanDays(getGamePlanString(game.plan));
  const startTime = Number(game.startTime) / 1000000; // Convert nanoseconds to milliseconds
  const endTime = startTime + (planDays * 24 * 60 * 60 * 1000);
  const remainingTime = Math.max(0, endTime - Date.now());
  
  const days = Math.floor(remainingTime / (1000 * 60 * 60 * 24));
  const hours = Math.floor((remainingTime % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
  const minutes = Math.floor((remainingTime % (1000 * 60 * 60)) / (1000 * 60));
  
  return { days, hours, minutes };
}

// Helper function to calculate Exit Toll fee
export function calculateExitTollFee(game: GameRecord, earnings: number): number {
  const startTime = Number(game.startTime) / 1000000; // Convert nanoseconds to milliseconds
  const elapsedTime = Date.now() - startTime;
  const elapsedDays = elapsedTime / (1000 * 60 * 60 * 24);

  if (game.isCompounding) {
    if ('compounding15Day' in game.plan) return earnings * JACKPOT_FEE_RATE_15D;
    return earnings * JACKPOT_FEE_RATE_30D;
  } else {
    // Tiered fees for simple mode
    if (elapsedDays < EXIT_TOLL_EARLY_DAYS) {
      return earnings * EXIT_TOLL_EARLY;
    } else if (elapsedDays < EXIT_TOLL_MID_DAYS) {
      return earnings * EXIT_TOLL_MID;
    } else {
      return earnings * EXIT_TOLL_LATE;
    }
  }
}

// Helper function to convert GamePlan variant to plan-id string.
// Candid variants come back as fresh objects ({ compounding30Day: null }),
// not as the static GamePlan.* references — switch-by-identity returned
// default for everything, which made all plans look like 21-day-simple.
function getGamePlanString(plan: GamePlan): string {
  if ('simple21Day' in plan) return '21-day-simple';
  if ('compounding15Day' in plan) return '15-day-compounding';
  if ('compounding30Day' in plan) return '30-day-compounding';
  return '21-day-simple';
}

// MLM stats — read from shenanigans, which holds the referral chain and
// accumulates per-tier PP earned by each upline. Falls back to zeros if the
// canister build pre-dates getReferralStats so the page still renders.
export function useGetReferralStats() {
  const { principal } = useWallet();
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['mlmStats', principal],
    queryFn: async () => {
      const empty = {
        level1Count: 0,
        level2Count: 0,
        level3Count: 0,
        level1Points: 0,
        level2Points: 0,
        level3Points: 0,
        totalEarnings: 0,
        referralLink: buildReferralLink(principal),
        recentSignups: [] as { principal: string; joinedAt: number; level: number }[],
      };
      if (!principal) return empty;
      try {
        const stats = await actor.getReferralStats(Principal.fromText(principal));
        const l1Points = ppUnitsToWhole(stats.l1Units);
        const l2Points = ppUnitsToWhole(stats.l2Units);
        const l3Points = ppUnitsToWhole(stats.l3Units);
        // recentSignups may be absent on canister builds that pre-date PR #36;
        // default to [] so the page renders.
        const rawSignups = (stats as unknown as { recentSignups?: { principal: Principal; joinedAt: bigint; level: bigint }[] }).recentSignups ?? [];
        const recentSignups = rawSignups.map((s) => ({
          principal: s.principal.toText(),
          joinedAt: Number(s.joinedAt / 1_000_000n), // ns → ms
          level: Number(s.level),
        }));
        return {
          level1Count: Number(stats.l1Count),
          level2Count: Number(stats.l2Count),
          level3Count: Number(stats.l3Count),
          level1Points: l1Points,
          level2Points: l2Points,
          level3Points: l3Points,
          totalEarnings: l1Points + l2Points + l3Points,
          referralLink: buildReferralLink(principal),
          recentSignups,
        };
      } catch (err) {
        console.warn('getReferralStats unavailable; showing zeros until backend deploys', err);
        return empty;
      }
    },
    enabled: !!principal,
    refetchInterval: 10000,
  });
}

// PP balances — read directly from pp_ledger (chip subaccount + user wallet)
export function useGetPonziPoints() {
  const ledger = useReadPpLedger();
  const { principal } = useWallet();

  return useQuery({
    queryKey: ['ppBalances', principal],
    queryFn: async () => {
      if (!principal) throw new Error('No principal');
      const p = Principal.fromText(principal);
      const [chipUnits, walletUnits] = await Promise.all([
        ledger.icrc1_balance_of({
          owner: shenanigansOwner(),
          subaccount: [principalToChipSubaccount(p)],
        }),
        ledger.icrc1_balance_of({ owner: p, subaccount: [] }),
      ]);
      return {
        chipPoints: ppUnitsToWhole(chipUnits),
        walletPoints: ppUnitsToWhole(walletUnits),
        totalPoints: ppUnitsToWhole(chipUnits + walletUnits),
      };
    },
    enabled: !!principal,
    refetchInterval: 5000,
  });
}

// Backer Positions Query
export function useGetBackerPositions() {
  const actor = useReadPonziMath();

  return useQuery<BackerPosition[]>({
    queryKey: ['backerPositions'],
    queryFn: async () => {
      try {
        const positions = await actor.getBackerPositions();
        return positions || [];
      } catch (error: any) {
        console.error('Failed to fetch backer positions:', error);
        return [];
      }
    },
    refetchInterval: 5000,
    retry: 2,
    retryDelay: 1000,
    placeholderData: [],
  });
}
// Legacy alias
export const useGetDealerPositions = useGetBackerPositions;

// Seed Round Dashboard Query
export function useGetSeedRoundDashboard() {
  const { data: backerPositions, isLoading: backersLoading } = useGetBackerPositions();

  return useQuery({
    queryKey: ['seedRoundDashboard', backerPositions],
    queryFn: async () => {
      try {
        const positions = backerPositions || [];

        const backersWithNames = positions.map((backer, index) => ({
          id: backer.owner.toString(),
          principal: backer.owner.toString(),
          // BackerPosition no longer has a name field — use generic label
          name: `Backer ${index + 1}`,
          entitlement: backer.entitlement,
          repaid: backer.entitlement - backer.amount,
          remaining: backer.amount,
          backerBonus: backer.entitlement - backer.amount,
          appointedAt: new Date(Number(backer.startTime) / 1000000),
        }));

        const totalOutstandingDebt = positions.reduce((sum, backer) => sum + backer.amount, 0);

        return {
          backers: backersWithNames,
          totalOutstandingDebt,
          seedVaultBalance: 0,
          repaymentPoolBalance: 0,
        };
      } catch (error: any) {
        console.error('Failed to process seed round dashboard data:', error);
        return {
          backers: [],
          totalOutstandingDebt: 0,
          seedVaultBalance: 0,
          repaymentPoolBalance: 0,
        };
      }
    },
    enabled: !backersLoading,
    refetchInterval: 5000,
    retry: 2,
    retryDelay: 1000,
    placeholderData: {
      backers: [],
      totalOutstandingDebt: 0,
      seedVaultBalance: 0,
      repaymentPoolBalance: 0,
    },
  });
}
// Legacy alias
export const useGetHouseDashboard = useGetSeedRoundDashboard;

// Top-holders leaderboard retired — pp_ledger exposes balances as on-chain state,
// not as a sorted view. The hook is kept as a no-op stub so existing consumers
// compile until Task 29 removes them.
export function useGetTopPonziPointsHolders() {
  return useQuery({
    queryKey: ['topPonziPointsHolders'],
    queryFn: async () => [] as { rank: number; name: string; ponziPoints: number; principal: string }[],
  });
}

export function useGetTopPonziPointsBurners() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['topPpBurners'],
    queryFn: async () => {
      const burners = await actor.getTopPpBurners(50n);
      return burners.map(([principal, unitsBig], index) => ({
        rank: index + 1,
        name: `User ${principal.toString().slice(-8)}`,
        ponziPointsBurned: Number(unitsBig / 100_000_000n),
        principal: principal.toString(),
      }));
    },
    refetchInterval: 30000,
  });
}

// Legacy Hall of Fame Query - kept for backward compatibility but deprecated
export function useGetHallOfFame() {
  const { data: holders } = useGetTopPonziPointsHolders();
  const { data: burners } = useGetTopPonziPointsBurners();

  return {
    data: { holders, burners },
    isLoading: false,
    error: null
  };
}

// === Chip custody & cash-out (pp_ledger + shenanigans) ===

const SHENANIGANS_PRINCIPAL = Principal.fromText('j56tm-oaaaa-aaaac-qf34q-cai');

/** Send whole-PP from the caller's main account to an arbitrary principal. */
export function useSendPp() {
  const ppLedger = useAuthPpLedger();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ to, wholePp }: { to: Principal; wholePp: number }) => {
      if (!ppLedger) throw new Error('No pp_ledger actor');
      const res = await ppLedger.icrc1_transfer({
        to: { owner: to, subaccount: [] },
        amount: wholePpToUnits(wholePp),
        fee: [],
        memo: [],
        from_subaccount: [],
        created_at_time: [],
      });
      if ('Err' in res) {
        const err: any = res.Err;
        const msg = err.InsufficientFunds ? 'Insufficient funds'
          : err.BadFee ? 'Bad fee'
          : err.TooOld ? 'Transfer too old'
          : err.TemporarilyUnavailable ? 'Ledger temporarily unavailable'
          : err.GenericError?.message || 'Transfer failed';
        throw new Error(msg);
      }
      return res.Ok;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['ppBalances'] }),
  });
}

/** One-time approve for chip deposits. Defaults to the ICRC-1 unlimited sentinel. */
export function useApproveForDeposits() {
  const ppLedger = useAuthPpLedger();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (explicitAmount?: bigint) => {
      if (!ppLedger) throw new Error('No pp_ledger actor');
      const UNLIMITED = 18_446_744_073_709_551_615n; // 2^64 - 1
      const amount = explicitAmount ?? UNLIMITED;
      const res = await ppLedger.icrc2_approve({
        from_subaccount: [],
        spender: { owner: SHENANIGANS_PRINCIPAL, subaccount: [] },
        amount,
        expected_allowance: [],
        expires_at: [],
        fee: [],
        memo: [],
        created_at_time: [],
      });
      if ('Err' in res) throw new Error(JSON.stringify(res.Err));
      return res.Ok;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['ppAllowance'] }),
  });
}

/** Current ICRC-2 allowance granted by the caller's main account to shenanigans. */
export function useAllowance() {
  const ppLedger = useReadPpLedger();
  const { principal } = useWallet();
  return useQuery({
    queryKey: ['ppAllowance', principal],
    queryFn: async () => {
      if (!principal) return null;
      const res = await ppLedger.icrc2_allowance({
        account: { owner: Principal.fromText(principal), subaccount: [] },
        spender: { owner: SHENANIGANS_PRINCIPAL, subaccount: [] },
      });
      return {
        allowance: res.allowance, // bigint, in PP-units
        expiresAt: res.expires_at[0] ?? null,
      };
    },
    enabled: !!principal,
    refetchInterval: 15000,
  });
}

/** Revoke by setting the allowance to 0. */
export function useRevokeAllowance() {
  const ppLedger = useAuthPpLedger();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      if (!ppLedger) throw new Error('No pp_ledger actor');
      const res = await ppLedger.icrc2_approve({
        from_subaccount: [],
        spender: { owner: SHENANIGANS_PRINCIPAL, subaccount: [] },
        amount: 0n,
        expected_allowance: [],
        expires_at: [],
        fee: [],
        memo: [],
        created_at_time: [],
      });
      if ('Err' in res) throw new Error(JSON.stringify(res.Err));
      return res.Ok;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['ppAllowance'] }),
  });
}

export function useDepositChips() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (wholePp: number) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.depositChips(wholePpToUnits(wholePp));
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['ppBalances'] });
      qc.invalidateQueries({ queryKey: ['ppAllowance'] });
    },
  });
}

export function useRequestCashOut() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (wholePp: number) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.requestCashOut(wholePpToUnits(wholePp));
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pendingCashOuts'] }),
  });
}

export function useClaimCashOut() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: bigint) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.claimCashOut(id);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pendingCashOuts'] });
      qc.invalidateQueries({ queryKey: ['ppBalances'] });
    },
  });
}

export function useCancelCashOut() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: bigint) => {
      if (!actor) throw new Error('No shenanigans actor');
      const res = await actor.cancelCashOut(id);
      if ('Err' in res) throw new Error(res.Err);
      return res.Ok;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pendingCashOuts'] });
      qc.invalidateQueries({ queryKey: ['ppBalances'] });
    },
  });
}

/** Live observer status — running/paused, cursors, interval. */
export function useGetObserverStatus() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['observerStatus'],
    queryFn: async () => actor.getObserverStatus(),
    refetchInterval: 5000,
  });
}

export function useStopObserver() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.stopObserver();
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['observerStatus'] }),
  });
}

export function useResumeObserver() {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.resumeObserver();
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['observerStatus'] }),
  });
}

/** Current mint config (observer interval, PP rates, referral BPS, cash-out delay). */
export function useGetMintConfig() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['mintConfig'],
    queryFn: async () => actor.getMintConfig(),
    refetchInterval: 30000,
  });
}

function useMintConfigSetter<Args extends unknown[]>(
  run: (actor: NonNullable<ReturnType<typeof useShenaniganActor>['actor']>, args: Args) => Promise<unknown>,
) {
  const { actor } = useShenaniganActor();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: Args) => {
      if (!actor) throw new Error('No shenanigans actor');
      return run(actor, args);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['mintConfig'] }),
  });
}

export const useSetSimple21 = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setSimple21DayPpPerIcp(v));
export const useSetCompounding15 = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setCompounding15DayPpPerIcp(v));
export const useSetCompounding30 = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setCompounding30DayPpPerIcp(v));
export const useSetDealerMultiplier = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setBackerPpPerIcp(v));
export const useSetReferralBps = () =>
  useMintConfigSetter<[bigint, bigint, bigint]>((a, [l1, l2, l3]) => a.setReferralBps(l1, l2, l3));
export const useSetMinDeposit = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setMinDepositPp(v));
export const useSetCashOutDelay = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setCashOutDelaySeconds(v));
export const useSetObserverInterval = () =>
  useMintConfigSetter<[bigint]>((a, [v]) => a.setObserverIntervalSeconds(v));

export function usePendingCashOuts() {
  const actor = useReadShenaniganActor();
  const { principal } = useWallet();
  return useQuery({
    queryKey: ['pendingCashOuts', principal],
    queryFn: async () => {
      if (!principal) return [];
      // Use the explicit-principal query so this stays anonymous on the wire —
      // otherwise Oisy would upgrade it to an icrc49 update call via the signer.
      const entries = await actor.getCashOutsFor(Principal.fromText(principal));
      return entries
        .filter((e) => !e.claimed)
        .map((e) => ({
          id: e.id,
          amount: Number(e.amount) / 1e8,
          claimableAfter: new Date(Number(e.claimableAfter) / 1_000_000),
          claimed: e.claimed,
        }));
    },
    enabled: !!principal,
    refetchInterval: 10000,
  });
}

// Ponzi Points calculation — uses live PP/ICP rates from shenanigans mintConfig.
export interface PpRates {
  simple21Day: number;
  comp15Day: number;
  comp30Day: number;
}

export function ppRateForPlan(plan: string, rates: PpRates): number {
  switch (plan) {
    case '21-day-simple': return rates.simple21Day;
    case '15-day-compounding': return rates.comp15Day;
    case '30-day-compounding': return rates.comp30Day;
    default: return 0;
  }
}

export function calculatePonziPoints(amount: number, plan: string, rates: PpRates): number {
  return amount * ppRateForPlan(plan, rates);
}

// Transaction History Query (Mock data until backend is implemented)
export function useGetTransactionHistory() {
  return useQuery({
    queryKey: ['transactionHistory'],
    queryFn: async () => {
      // TODO: wire to useReadActor + actor.getTransactionHistory() when backend method exists.
      
      // Mock data for now - simulating live transaction history
      const baseTransactions = [
        {
          id: 'tx_1',
          type: 'deposit' as const,
          amount: 100,
          date: new Date('2025-01-20'),
          status: 'completed' as const,
          houseFee: 3.0,
        },
        {
          id: 'tx_2',
          type: 'game' as const,
          amount: 50,
          date: new Date('2025-01-21'),
          status: 'completed' as const,
          planId: '21-day-simple',
          ponziPoints: 50000,
        },
        {
          id: 'tx_3',
          type: 'withdrawal' as const,
          amount: 25,
          date: new Date('2025-01-22'),
          status: 'completed' as const,
          exitTollFee: 25 * EXIT_TOLL_EARLY,
        },
        {
          id: 'tx_4',
          type: 'mlm_reward' as const,
          amount: 10,
          date: new Date('2025-01-23'),
          status: 'completed' as const,
          description: 'Level 1 MLM reward (8%)'
        }
      ];

      // Simulate new transactions occasionally
      if (Math.random() < 0.2) { // 20% chance of new transaction
        const randomChoice = Math.random();
        
        if (randomChoice < 0.33) {
          // Add deposit transaction
          baseTransactions.unshift({
            id: `tx_${Date.now()}`,
            type: 'deposit' as const,
            amount: Math.floor(Math.random() * 100) + 10,
            date: new Date(),
            status: 'completed' as const,
            houseFee: (Math.floor(Math.random() * 100) + 10) * 0.03,
          });
        } else if (randomChoice < 0.66) {
          // Add withdrawal transaction
          const amount = Math.floor(Math.random() * 50) + 5;
          baseTransactions.unshift({
            id: `tx_${Date.now()}`,
            type: 'withdrawal' as const,
            amount,
            date: new Date(),
            status: 'completed' as const,
            exitTollFee: amount * EXIT_TOLL_EARLY,
          });
        } else {
          // Add MLM reward transaction
          const level = Math.floor(Math.random() * 3) + 1;
          baseTransactions.unshift({
            id: `tx_${Date.now()}`,
            type: 'mlm_reward' as const,
            amount: Math.floor(Math.random() * 20) + 5,
            date: new Date(),
            status: 'completed' as const,
            description: `Level ${level} MLM reward (${level === 1 ? '8%' : level === 2 ? '5%' : '2%'})`
          });
        }
      }

      return baseTransactions;
    },
  });
}

// ============================================================================
// New hooks added during ponzi_math extraction
// ============================================================================

// Legacy alias — HouseDashboard imports useClaimDealerRepayment
export const useClaimDealerRepayment = useClaimBackerRepayment;

// Backend ICP balance — admin-only diagnostic. The canister enforces auth.
export function useBackendICPBalance() {
  const { actor, isFetching: actorFetching } = useActor();
  const { principal } = useWallet();

  return useQuery<bigint>({
    queryKey: ['backendICPBalance'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getBackendICPBalance();
    },
    enabled: !!actor && !actorFetching && !!principal && isCoverChargeAdmin(principal),
    refetchInterval: 10000,
  });
}

// ============================================================================
// Admin god-view queries — caller-gated by ponzi_math.isAdmin(). Querying with
// a non-admin identity TRAPS; React Query catches the rejection and surfaces
// as `error`. The `enabled` guards below restrict client-side requests to
// Charles principals so non-admin users never see the trap.
// ============================================================================

// useReadPonziMath() builds an actor with an ANONYMOUS HttpAgent (no identity),
// so the canister sees Principal.anonymous() as the caller — that fails
// requireAuthenticated before requireAdmin runs. We need the identity-bearing
// actor (usePonziMathActor) for admin queries to send the signed envelope.

export function useAdminIsAdmin() {
  const { actor } = usePonziMathActor();
  const { principal } = useWallet();
  return useQuery<boolean>({
    queryKey: ['adminIsAdmin', principal],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.adminIsAdmin();
    },
    enabled: !!actor && !!principal,
    staleTime: 60_000,
  });
}

export function useAdminGetCurrentRoundId(enabled: boolean = true) {
  const { actor } = usePonziMathActor();
  return useQuery<bigint>({
    queryKey: ['adminCurrentRoundId'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.adminGetCurrentRoundId();
    },
    enabled: enabled && !!actor,
    refetchInterval: 10_000,
  });
}

export function useAdminGetActivePlansSnapshot(enabled: boolean = true) {
  const { actor } = usePonziMathActor();
  return useQuery<ActivePlanSnapshot[]>({
    queryKey: ['adminActivePlansSnapshot'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.adminGetActivePlansSnapshot();
    },
    enabled: enabled && !!actor,
    refetchInterval: 5_000,
  });
}

export function useAdminGetRoundSummaries(enabled: boolean = true) {
  const { actor } = usePonziMathActor();
  return useQuery<RoundSummary[]>({
    queryKey: ['adminRoundSummaries'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.adminGetRoundSummaries();
    },
    enabled: enabled && !!actor,
    refetchInterval: 30_000,
  });
}

export function useAdminGetEventsByRound(roundId: bigint | null, enabled: boolean = true) {
  const { actor } = usePonziMathActor();
  return useQuery<GeneralLedgerEntry[]>({
    queryKey: ['adminEventsByRound', roundId?.toString()],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      if (roundId === null) throw new Error('No round selected');
      return actor.adminGetEventsByRound(roundId);
    },
    enabled: enabled && !!actor && roundId !== null,
    refetchInterval: 5_000,
  });
}

export function useAdminGetEventsForGame(gameId: bigint | null) {
  const { actor } = usePonziMathActor();
  return useQuery<GeneralLedgerEntry[]>({
    queryKey: ['adminEventsForGame', gameId?.toString()],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      if (gameId === null) throw new Error('No game selected');
      return actor.adminGetEventsForGame(gameId);
    },
    enabled: !!actor && gameId !== null,
    staleTime: 10_000,
  });
}

// Lookup a single profile by principal text. Used by admin views to resolve
// game/event owner principals to display names. Backed by getUserProfile on
// the backend canister, which returns null for unregistered principals.
export function useGetProfileFor(principalText: string | undefined) {
  const actor = useReadActor();
  return useQuery<UserProfile | null>({
    queryKey: ['profileFor', principalText],
    queryFn: async () => {
      if (!principalText) return null;
      const result = await actor.getUserProfile(Principal.fromText(principalText));
      return result[0] ?? null;
    },
    enabled: !!principalText,
    staleTime: 5 * 60_000,
  });
}

// Best-effort: register the stored referrer with shenanigans for `principal`.
// Idempotent on the canister side (first-wins) and short-circuits with a
// query if the chain entry already exists, so it's safe and cheap to call
// repeatedly. Swallows errors and logs them — callers should never have
// their flow blocked by a referral failure.
//
// Used by both the auto-register hook (fires on auth) AND the deposit flow
// (awaited before createGame). Awaiting before createGame is load-bearing:
// the shenanigans observer reads referralChain at the moment it mints the
// signup gift, so the chain MUST be set on-canister before the new game
// becomes visible to the observer, or the gift cascade goes to "house"
// instead of up the chain.
async function ensureReferralRegistered(
  authActor: { registerReferral: (p: Principal) => Promise<undefined> },
  readActor: {
    getReferrer: (p: Principal) => Promise<[] | [Principal]>;
    resolveReferralCode: (code: string) => Promise<[] | [Principal]>;
  },
  principal: string,
): Promise<void> {
  const stored = getStoredReferrer();
  if (!stored || stored === principal) return;
  // Charles is the top of the chain by design — never register him as someone
  // else's downline, even if a stale ?ref= code is sitting in localStorage.
  if (isCharles(principal)) return;

  try {
    // Skip if already registered — avoids redundant update calls (and an
    // unnecessary Oisy popup on every deposit).
    const existing = await readActor.getReferrer(Principal.fromText(principal));
    if (existing.length > 0) return;

    let referrerPrincipal: Principal;
    try {
      referrerPrincipal = Principal.fromText(stored);
    } catch {
      // Resolve via the anonymous read actor so Oisy doesn't open a signer
      // popup for what's a public lookup.
      const resolved = await readActor.resolveReferralCode(stored);
      if (resolved.length === 0) {
        console.warn('[referral] Unknown code, skipping registration:', stored);
        return;
      }
      referrerPrincipal = resolved[0];
    }
    await authActor.registerReferral(referrerPrincipal);
  } catch (err) {
    console.error('[referral] register failed:', err);
  }
}

// Auto-register on auth — fires once when the wallet and shenanigans auth
// actor are both ready. The actor-readiness gate is load-bearing: without
// it, the effect would race the async actor creation in useShenaniganActor
// and fire while `actor` is still null. The deposit flow also calls
// ensureReferralRegistered to close the remaining race against the
// shenanigans observer (see useCreateGame).
export function useRegisterReferral() {
  const { actor } = useShenaniganActor();
  const readActor = useReadShenaniganActor();
  const { isConnected, principal } = useWallet();
  const hasRegisteredRef = useRef(false);

  useEffect(() => {
    if (!isConnected || !actor || !principal || hasRegisteredRef.current) return;
    hasRegisteredRef.current = true;
    ensureReferralRegistered(actor, readActor, principal).catch(() => {
      hasRegisteredRef.current = false; // allow retry on next deps change
    });
  }, [isConnected, principal, actor, readActor]);
}

// Issue (or fetch existing) the caller's short referral code, then build the
// share URL. One round-trip per session; cached by React Query keyed on
// principal. The canister hands back the same code forever once issued.
export function useGetMyReferralCode() {
  const { actor, isFetching: actorFetching } = useShenaniganActor();
  const { principal } = useWallet();
  return useQuery({
    queryKey: ['myReferralCode', principal],
    queryFn: async () => {
      if (!actor) throw new Error('Shenanigans actor not available');
      const code = await actor.getOrCreateReferralCode();
      return { code, link: buildReferralLink(code) };
    },
    enabled: !!principal && !!actor && !actorFetching,
    staleTime: Infinity,
  });
}

// Walk the referral chain upward from the current user — returns the chain
// from immediate referrer outward (index 0 = L-1, index 1 = L-2, ...). Bounded
// to 3 hops; stops early at chain root (the house). Each call is a query, so
// a 3-deep chain is 3 round-trips. Cached aggressively (immutable chain).
export function useGetUplineChain(maxDepth: number = 3) {
  const { principal } = useWallet();
  const actor = useReadShenaniganActor();
  return useQuery<string[]>({
    queryKey: ['uplineChain', principal, maxDepth],
    queryFn: async () => {
      if (!principal) return [];
      const chain: string[] = [];
      let current = Principal.fromText(principal);
      for (let i = 0; i < maxDepth; i++) {
        try {
          const result = await actor.getReferrer(current);
          if (result.length === 0) break;
          const referrer = result[0];
          chain.push(referrer.toText());
          current = referrer;
        } catch {
          break;
        }
      }
      return chain;
    },
    enabled: !!principal,
    staleTime: 5 * 60_000,
  });
}

// ============================================================================
// Trollbox hooks — chat, reactions, admin moderation
// ============================================================================

export function useRecentChatItems() {
  const actor = useReadShenaniganActor();
  return useQuery<ChatItem[]>({
    queryKey: ['shenanigans', 'chatItems'],
    queryFn: async () => actor.getRecentChatItems(BigInt(TROLLBOX_FETCH_LIMIT)),
    refetchInterval: TROLLBOX_POLL_MS,
  });
}

export function useCurrentPin() {
  const actor = useReadShenaniganActor();
  return useQuery<ChatItem | null>({
    queryKey: ['shenanigans', 'currentPin'],
    queryFn: async () => {
      const result = await actor.getCurrentPin();
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: TROLLBOX_PIN_POLL_MS,
  });
}

export function useIsMuted(principal: Principal | null) {
  const actor = useReadShenaniganActor();
  return useQuery<bigint | null>({
    queryKey: ['shenanigans', 'isMuted', principal?.toText()],
    queryFn: async () => {
      if (!principal) return null;
      const result = await actor.isMuted(principal);
      return result.length === 0 ? null : result[0];
    },
    refetchInterval: TROLLBOX_MUTE_POLL_MS,
    enabled: !!principal,
  });
}

export function usePostChatMessage() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ body, replyTo }: { body: string; replyTo: bigint | null }) => {
      if (!actor) throw new Error('No shenanigans actor');
      const reply: [] | [bigint] = replyTo === null ? [] : [replyTo];
      const result = await actor.postChatMessage(body, reply);
      if ('Err' in result) throw new Error(result.Err);
      return result.Ok;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useAddReaction() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ itemId, emoji }: { itemId: bigint; emoji: string }) => {
      if (!actor) throw new Error('No shenanigans actor');
      const result = await actor.addReaction(itemId, emoji);
      if ('Err' in result) throw new Error(result.Err);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useRemoveReaction() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ itemId, emoji }: { itemId: bigint; emoji: string }) => {
      if (!actor) throw new Error('No shenanigans actor');
      const result = await actor.removeReaction(itemId, emoji);
      if ('Err' in result) throw new Error(result.Err);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useKarmaReact() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ itemId, emoji, ppToBurn }: { itemId: bigint; emoji: string; ppToBurn: bigint }) => {
      if (!actor) throw new Error('No shenanigans actor');
      const result = await actor.addKarmaReaction(itemId, emoji, ppToBurn);
      if ('Err' in result) throw new Error(result.Err);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'karmaReceived'] });
    },
  });
}

export function useGetKarmaReceived(principalText?: string) {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'karmaReceived', principalText],
    queryFn: async () => {
      if (!actor || !principalText) return 0n;
      return actor.getKarmaReceived(Principal.fromText(principalText));
    },
    enabled: !!actor && !!principalText,
    refetchInterval: 15000,
  });
}

export function useAdminSetPin() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: string) => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.adminSetPin(body);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'currentPin'] });
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useAdminDeleteChatItem() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (itemId: bigint) => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.adminDeleteChatItem(itemId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useAdminMuteUser() {
  const { actor } = useShenaniganActor();
  return useMutation({
    mutationFn: async ({ user, durationSeconds }: { user: Principal; durationSeconds: bigint }) => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.adminMuteUser(user, durationSeconds);
    },
  });
}

export function useAdminUnmute() {
  const { actor } = useShenaniganActor();
  return useMutation({
    mutationFn: async (user: Principal) => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.adminUnmute(user);
    },
  });
}

export function useAdminPostAsReginald() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (line: string) => {
      if (!actor) throw new Error('No shenanigans actor');
      return actor.adminPostAsReginald(line);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chatItems'] });
    },
  });
}

export function useListChimeSounds() {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'chimeSounds'],
    queryFn: async () => actor.listChimeSounds(),
    refetchInterval: 60_000,
    enabled: !!actor,
  });
}

export function useGetChimeSound(name: string | null) {
  const actor = useReadShenaniganActor();
  return useQuery({
    queryKey: ['shenanigans', 'chimeSound', name],
    queryFn: async () => {
      if (!name) return null;
      const result = await actor.getChimeSound(name);
      return result.length === 0 ? null : result[0];
    },
    enabled: !!actor && !!name,
    staleTime: 60 * 60_000, // Bytes don't change without an admin upload; cache aggressively.
  });
}

export function useAdminUploadChimeSound() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ name, mimeType, bytes }: { name: string; mimeType: string; bytes: Uint8Array | number[] }) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      const result = await actor.adminUploadChimeSound(name, mimeType, bytes);
      if ('Err' in result) throw new Error(result.Err);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chimeSounds'] });
    },
  });
}

export function useAdminDeleteChimeSound() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      await actor.adminDeleteChimeSound(name);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'chimeSounds'] });
    },
  });
}

export function useListFlavorPools() {
  const actor = useReadShenaniganActor();
  return useQuery<Array<[string, string[]]>>({
    queryKey: ['shenanigans', 'flavorPools'],
    queryFn: async () => actor.listFlavorPools() as Promise<Array<[string, string[]]>>,
    refetchInterval: 30_000,
    enabled: !!actor,
  });
}

export function useGetFlavorPoolDefaults(name: string | null) {
  const actor = useReadShenaniganActor();
  return useQuery<string[]>({
    queryKey: ['shenanigans', 'flavorPoolDefaults', name],
    queryFn: async () => {
      if (!name) return [];
      return actor.getFlavorPoolDefaults(name);
    },
    enabled: !!actor && !!name,
    staleTime: Infinity,  // Defaults are part of canister code; never change between deploys.
  });
}

export function useAdminSetFlavorPool() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ name, lines }: { name: string; lines: string[] }) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      await actor.adminSetFlavorPool(name, lines);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'flavorPools'] });
    },
  });
}

export function useAdminClearFlavorPool() {
  const { actor } = useShenaniganActor();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      if (!actor) throw new Error('Shenanigans actor not available');
      await actor.adminClearFlavorPool(name);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shenanigans', 'flavorPools'] });
    },
  });
}
