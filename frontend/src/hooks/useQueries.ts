import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useActor } from './useActor';
import { useWallet } from './useWallet';
import { UserProfile, GameRecord, GamePlan, PlatformStats, ShenaniganType, ShenaniganOutcome, ShenaniganStats, ShenaniganRecord, DealerPosition as BackerPosition, HouseLedgerRecord, ShenaniganConfig } from '../backend';
// Re-export backend's DealerPosition as BackerPosition for the rest of the app
export type { BackerPosition };
import { Principal } from '@dfinity/principal';
import { Actor, HttpAgent } from '@dfinity/agent';
import { idlFactory } from '../declarations/backend';
import type { _SERVICE } from '../declarations/backend';

const BACKEND_CANISTER_ID = '5zxxg-tyaaa-aaaac-qeckq-cai';
const HOST = 'https://icp0.io';

// User Profile Queries
export function useGetCallerUserProfile() {
  const { actor, isFetching: actorFetching } = useActor();

  const query = useQuery<UserProfile | null>({
    queryKey: ['currentUserProfile'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      const result = await actor.getCallerUserProfile();
      // Convert Candid optional ([] | [UserProfile]) to UserProfile | null
      return result.length > 0 ? result[0] : null;
    },
    enabled: !!actor && !actorFetching,
    retry: false,
  });

  return {
    ...query,
    isLoading: actorFetching || query.isLoading,
    isFetched: !!actor && query.isFetched,
  };
}

export function useSaveUserProfile() {
  const { actor } = useActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (profile: UserProfile) => {
      if (!actor) throw new Error('Actor not available');
      return actor.saveCallerUserProfile(profile);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['currentUserProfile'] });
    },
  });
}

// Game Statistics Queries
export function useGetGameStats() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<PlatformStats>({
    queryKey: ['gameStats'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getPlatformStats();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
  });
}

// Public stats — no auth required, uses anonymous actor for splash page
export function useGetPublicStats() {
  return useQuery<PlatformStats>({
    queryKey: ['publicStats'],
    queryFn: async () => {
      const agent = new HttpAgent({ host: HOST });
      const anonActor = Actor.createActor<_SERVICE>(idlFactory, {
        agent,
        canisterId: BACKEND_CANISTER_ID,
      });
      return anonActor.getPlatformStats();
    },
    refetchInterval: 30000,
    staleTime: 15000,
  });
}

// Deposit Limits and Rate Limiting Queries
export function useGetMaxDepositLimit() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<number>({
    queryKey: ['maxDepositLimit'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getMaxDepositLimit();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds as pot balance changes
  });
}

export function useCheckDepositRateLimit() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<boolean>({
    queryKey: ['depositRateLimit'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.checkDepositRateLimit();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 60000, // Refetch every minute to update rate limit status
  });
}

// Backer Repayment Balance Query
export function useGetBackerRepaymentBalance() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<number>({
    queryKey: ['backerRepaymentBalance'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getDealerRepaymentBalance(); // Backend Candid name unchanged
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000,
  });
}
// Legacy alias
export const useGetHouseRepaymentBalance = useGetBackerRepaymentBalance;

// House Ledger Queries with enhanced error handling
export function useGetHouseLedger() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<HouseLedgerRecord[]>({
    queryKey: ['houseLedger'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      try {
        const records = await actor.getHouseLedger();
        return records || [];
      } catch (error: any) {
        console.error('Failed to fetch house ledger:', error);
        // Return empty array instead of throwing to prevent crashes
        return [];
      }
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
    retry: 2,
    retryDelay: 1000,
    // Provide fallback data to prevent crashes
    placeholderData: [],
  });
}

export function useGetHouseLedgerStats() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['houseLedgerStats'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      try {
        const stats = await actor.getHouseLedgerStats();
        return stats || {
          totalDeposits: 0,
          totalWithdrawals: 0,
          netBalance: 0,
          recordCount: BigInt(0)
        };
      } catch (error: any) {
        console.error('Failed to fetch house ledger stats:', error);
        // Return default stats instead of throwing to prevent crashes
        return {
          totalDeposits: 0,
          totalWithdrawals: 0,
          netBalance: 0,
          recordCount: BigInt(0)
        };
      }
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
    retry: 2,
    retryDelay: 1000,
    // Provide fallback data to prevent crashes
    placeholderData: {
      totalDeposits: 0,
      totalWithdrawals: 0,
      netBalance: 0,
      recordCount: BigInt(0)
    },
  });
}

// Musical Chairs Wallet Queries - Now using real backend
export function useGetInternalWalletBalance() {
  const { actor, isFetching: actorFetching } = useActor();
  const { principal, isConnected, walletType } = useWallet();
  const { data: houseRepaymentBalance } = useGetBackerRepaymentBalance();

  return useQuery({
    queryKey: ['internalWalletBalance', principal, houseRepaymentBalance],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      // Get wallet balance from backend (in ICP, not e8s)
      const balanceICP = await actor.getWalletBalanceICP();
      const houseRepayments = houseRepaymentBalance || 0;
      
      // Check if test mode is enabled
      const isTestMode = await actor.isTestMode();
      
      return {
        internalBalance: balanceICP + houseRepayments,
        houseRepaymentBalance: houseRepayments,
        walletType,
        isTestMode,
      };
    },
    enabled: !!actor && !actorFetching && !!principal && isConnected,
    refetchInterval: 5000, // Refetch every 5 seconds for live balance updates
  });
}

// ============================================================================
// Real ICP Deposit/Withdraw Mutations
// ============================================================================

// Deposit ICP from external wallet to Musical Chairs (ICRC-2 transfer_from)
// User must first call icrc2_approve on the ICP ledger to authorize the backend
export function useDepositICP() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (amountE8s: bigint) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      if (amountE8s < 10_000_000n) {
        throw new Error('Minimum deposit is 0.1 ICP');
      }
      
      // Call backend to pull funds via ICRC-2 transfer_from
      const result = await actor.depositICP(amountE8s);
      
      if ('Err' in result) {
        throw new Error(result.Err);
      }
      
      return { 
        success: true, 
        blockIndex: result.Ok,
        amount: Number(amountE8s) / 100_000_000,
        timestamp: new Date()
      };
    },
    onSuccess: () => {
      // Immediately invalidate and refetch balance queries
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
    },
  });
}

// Withdraw ICP from Musical Chairs to external wallet (ICRC-1 transfer)
export function useWithdrawICP() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (amountE8s: bigint) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      if (amountE8s < 10_000_000n) {
        throw new Error('Minimum withdrawal is 0.1 ICP');
      }
      
      // Call backend to send ICP to user's wallet
      const result = await actor.withdrawICP(amountE8s);
      
      if ('Err' in result) {
        throw new Error(result.Err);
      }
      
      return { 
        success: true, 
        blockIndex: result.Ok,
        amount: Number(amountE8s) / 100_000_000,
        timestamp: new Date()
      };
    },
    onSuccess: () => {
      // Immediately invalidate and refetch balance queries
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
    },
  });
}

export function useSendFromInternalWallet() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ amount, recipientPrincipal }: { amount: number; recipientPrincipal: string }) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      // Validate send amount
      if (amount <= 0) {
        throw new Error('Send amount must be greater than 0');
      }
      
      // Convert ICP to e8s for the backend
      const amountE8s = BigInt(Math.round(amount * 100_000_000));
      
      // Call backend to transfer
      const result = await actor.transferInternal(
        Principal.fromText(recipientPrincipal),
        amountE8s
      );
      
      if ('Err' in result) {
        throw new Error(result.Err);
      }
      
      return { 
        success: true, 
        transactionId: `send_${Date.now()}`,
        amount,
        recipient: recipientPrincipal,
        timestamp: new Date()
      };
    },
    onSuccess: () => {
      // Immediately invalidate and refetch balance queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
    },
  });
}

// Add Backer Money Mutation — regular users become Series A backers
export function useAddBackerMoney() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (amount: number) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      // Get current wallet balance from backend
      const currentBalance = await actor.getWalletBalanceICP();
      
      if (amount > currentBalance) {
        throw new Error('Insufficient balance in Musical Chairs Wallet. Please fund your wallet first.');
      }
      
      try {
        // Call backend to add backer money (this will deduct from internal wallet)
        await actor.addDealerMoney(amount); // Backend Candid name unchanged

        return {
          success: true,
          amount,
          expectedReturn: amount * 1.12,
          ponziPoints: amount * 4000, // 4000 points per ICP for backer money
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
      queryClient.invalidateQueries({ queryKey: ['houseLedger'] });
      queryClient.invalidateQueries({ queryKey: ['houseLedgerStats'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.refetchQueries({ queryKey: ['backerPositions'] });
      queryClient.refetchQueries({ queryKey: ['houseLedger'] });
      queryClient.refetchQueries({ queryKey: ['houseLedgerStats'] });
    },
  });
}
// Legacy alias
export const useAddDealerMoney = useAddBackerMoney;

// Add House Money Mutation with enhanced error handling (for admin use)
export function useAddHouseMoney() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (amount: number) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      // Get current wallet balance from backend
      const currentBalance = await actor.getWalletBalanceICP();
      
      if (amount > currentBalance) {
        throw new Error('Insufficient balance in Musical Chairs Wallet. Please fund your wallet first.');
      }
      
      try {
        // Call backend to add house money with description
        await actor.addHouseMoney(amount, `House money deposit of ${amount} ICP`);
        
        return { 
          success: true, 
          amount,
          expectedReturn: amount * 1.12,
          ponziPoints: amount * 4000, // Updated to 4000 points per ICP for house money
          timestamp: new Date()
        };
      } catch (error: any) {
        console.error('Failed to add house money:', error);
        // Provide more user-friendly error messages
        if (error.message?.includes('Unauthorized')) {
          throw new Error('You do not have permission to add house money. Admin access required.');
        } else if (error.message?.includes('Amount must be greater than 0')) {
          throw new Error('Please enter a valid amount greater than 0.');
        } else {
          throw new Error('Failed to add house money. Please try again or contact support.');
        }
      }
    },
    onSuccess: () => {
      // Immediately invalidate and refetch all related queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
      queryClient.invalidateQueries({ queryKey: ['backerPositions'] });
      queryClient.invalidateQueries({ queryKey: ['houseLedger'] });
      queryClient.invalidateQueries({ queryKey: ['houseLedgerStats'] });
      queryClient.invalidateQueries({ queryKey: ['gameStats'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.refetchQueries({ queryKey: ['internalWalletBalance'] });
      queryClient.refetchQueries({ queryKey: ['backerPositions'] });
      queryClient.refetchQueries({ queryKey: ['houseLedger'] });
      queryClient.refetchQueries({ queryKey: ['houseLedgerStats'] });
    },
  });
}

// Store for tracking accumulated earnings per game
const gameEarningsStore = new Map<string, { lastUpdateTime: number; accumulatedEarnings: number }>();

// Game Queries with manual refresh functionality
export function useGetUserGames() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<GameRecord[]>({
    queryKey: ['userGames'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      const games = await actor.getUserGames();
      
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
    enabled: !!actor && !actorFetching,
    // Remove automatic refetch interval - now manual only
  });
}

export function useCreateGame() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ planId, amount, mode }: { planId: string; amount: number; mode: 'simple' | 'compounding' }) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      // Get current wallet balance from backend
      const currentBalance = await actor.getWalletBalanceICP();
      
      if (amount > currentBalance) {
        throw new Error('Insufficient balance in Musical Chairs Wallet. Please fund your wallet first.');
      }
      
      // Convert planId to GamePlan enum based on new plan structure
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
      
      const isCompounding = mode === 'compounding';
      const gameId = await actor.createGame(plan, amount, isCompounding, null);
      
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
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
    },
  });
}

export function useWithdrawGameEarnings() {
  const { actor } = useActor();
  const { principal } = useWallet();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (gameId: bigint) => {
      if (!actor) throw new Error('Actor not available');
      if (!principal) throw new Error('Not authenticated');
      
      const earnings = await actor.withdrawEarnings(gameId);
      
      // Reset accumulated earnings for this game after withdrawal
      gameEarningsStore.delete(gameId.toString());
      
      // Calculate Exit Toll fee (mock implementation)
      // TODO: Replace with actual backend calculation
      const exitTollFee = earnings * 0.07; // Assuming 7% for demo
      const netEarnings = earnings - exitTollFee;
      
      // Add net earnings to Musical Chairs Wallet balance
      initializeWalletIfNeeded(principal);
      const currentBalance = simulatedMusicalChairsWallets.get(principal) || 500.0;
      simulatedMusicalChairsWallets.set(principal, currentBalance + netEarnings);
      
      return { earnings, netEarnings, exitTollFee, gameId };
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

export function useCalculateGameEarnings() {
  const { actor } = useActor();

  return useMutation({
    mutationFn: async (game: GameRecord) => {
      if (!actor) throw new Error('Actor not available');
      return actor.calculateEarnings(game);
    },
  });
}

// Shenanigans Queries
export function useGetShenaniganStats() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<ShenaniganStats>({
    queryKey: ['shenaniganStats'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getShenaniganStats();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
  });
}

export function useGetRecentShenanigans() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<ShenaniganRecord[]>({
    queryKey: ['recentShenanigans'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getRecentShenanigans();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 3000, // Refetch every 3 seconds for live feed updates
  });
}

export function useCastShenanigan() {
  const { actor } = useActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ shenaniganType, target }: { shenaniganType: ShenaniganType; target: Principal | null }) => {
      if (!actor) throw new Error('Actor not available');
      return actor.castShenanigan(shenaniganType, target);
    },
    onSuccess: () => {
      // Invalidate and refetch all related queries for instant UI updates
      queryClient.invalidateQueries({ queryKey: ['shenaniganStats'] });
      queryClient.invalidateQueries({ queryKey: ['recentShenanigans'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPoints'] });
      queryClient.invalidateQueries({ queryKey: ['ponziPointsBalance'] });
      queryClient.invalidateQueries({ queryKey: ['houseRepaymentBalance'] });
    },
  });
}

// Shenanigans Configuration Queries
export function useGetShenaniganConfigs() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<ShenaniganConfig[]>({
    queryKey: ['shenaniganConfigs'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      return actor.getShenaniganConfigs();
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 10000, // Refetch every 10 seconds for updates
  });
}

export function useUpdateShenaniganConfig() {
  const { actor } = useActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (config: ShenaniganConfig) => {
      if (!actor) throw new Error('Actor not available');
      return actor.updateShenaniganConfig(config);
    },
    onSuccess: () => {
      // Invalidate and refetch shenanigan configs
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
      queryClient.refetchQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useSaveAllShenaniganConfigs() {
  const { actor } = useActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (configs: ShenaniganConfig[]) => {
      if (!actor) throw new Error('Actor not available');
      return actor.saveAllShenaniganConfigs(configs);
    },
    onSuccess: () => {
      // Invalidate and refetch shenanigan configs
      queryClient.invalidateQueries({ queryKey: ['shenaniganConfigs'] });
      queryClient.refetchQueries({ queryKey: ['shenaniganConfigs'] });
    },
  });
}

export function useResetShenaniganConfig() {
  const { actor } = useActor();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: bigint) => {
      if (!actor) throw new Error('Actor not available');
      return actor.resetShenaniganConfig(id);
    },
    onSuccess: () => {
      // Invalidate and refetch shenanigan configs
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

// Helper function to calculate incremental earnings for a time period
function calculateIncrementalEarnings(game: GameRecord, elapsedSeconds: number): number {
  const dailyRate = getDailyRate(getGamePlanString(game.plan));
  
  if (game.isCompounding) {
    // For compounding, use the updated formula: earnings = principal × [(1 + daily_rate) ^ (elapsed_seconds / 86400) - 1]
    // This calculates only the compounded interest, not including the principal
    const elapsedDays = elapsedSeconds / 86400;
    const compoundedInterest = game.amount * (Math.pow(1 + dailyRate, elapsedDays) - 1);
    
    // Return incremental earnings since last update
    const stored = gameEarningsStore.get(game.id.toString());
    const previousEarnings = stored ? stored.accumulatedEarnings : game.accumulatedEarnings;
    
    return Math.max(0, compoundedInterest - previousEarnings);
  } else {
    // Simple formula: earnings = principal × rate × (elapsed_seconds / 86400)
    // Updated to use the new 11% daily rate for simple mode
    return game.amount * dailyRate * (elapsedSeconds / 86400);
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
    return earnings * 0.13; // Flat 13% for compounding
  } else {
    // Tiered fees for simple mode
    if (elapsedDays < 3) {
      return earnings * 0.07; // 7% within 3 days
    } else if (elapsedDays < 10) {
      return earnings * 0.05; // 5% within 10 days
    } else {
      return earnings * 0.03; // 3% after 10 days
    }
  }
}

// Helper function to convert GamePlan enum to string
function getGamePlanString(plan: GamePlan): string {
  switch (plan) {
    case GamePlan.simple21Day:
      return '21-day-simple';
    case GamePlan.compounding15Day:
      return '15-day-compounding';
    case GamePlan.compounding30Day:
      return '30-day-compounding';
    default:
      return '21-day-simple';
  }
}

// MLM Queries - Updated to use real backend data
export function useGetReferralStats() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['mlmStats'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      
      // Get real referral tier points from backend
      const tierPoints = await actor.getReferralTierPoints();
      
      return {
        level1Count: 0, // Backend doesn't track count, only points
        level2Count: 0,
        level3Count: 0,
        level1Points: tierPoints.level1Points,
        level2Points: tierPoints.level2Points,
        level3Points: tierPoints.level3Points,
        totalEarnings: tierPoints.totalPoints,
        referralLink: `https://musical-chairs.com/ref/${Date.now().toString(36)}`
      };
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
  });
}

// Ponzi Points Queries - Updated to use real backend data
export function useGetPonziPoints() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['ponziPointsBalance'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      
      // Get real Ponzi Points balance from backend
      const balance = await actor.getPonziPointsBalance();
      
      return {
        totalPoints: balance.totalPoints,
        depositPoints: balance.depositPoints,
        referralPoints: balance.referralPoints,
      };
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 5000, // Refetch every 5 seconds for live updates
  });
}

// Backer Positions Query
export function useGetBackerPositions() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery<BackerPosition[]>({
    queryKey: ['backerPositions'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      try {
        const positions = await actor.getDealerPositions(); // Backend Candid name unchanged
        return positions || [];
      } catch (error: any) {
        console.error('Failed to fetch backer positions:', error);
        return [];
      }
    },
    enabled: !!actor && !actorFetching,
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
  const { actor } = useActor();

  return useQuery({
    queryKey: ['seedRoundDashboard', backerPositions],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');

      try {
        const positions = backerPositions || [];

        const backersWithNames = positions.map((backer, index) => ({
          id: backer.owner.toString(),
          principal: backer.owner.toString(),
          name: backer.name || `Backer ${index + 1}`,
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
    enabled: !!actor && !backersLoading,
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

// Hall of Fame Queries - Updated to use separate backend calls
export function useGetTopPonziPointsHolders() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['topPonziPointsHolders'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      const holders = await actor.getTopPonziPointsHolders();
      
      // Transform backend data to include user names
      return holders.map(([principal, points], index) => ({
        rank: index + 1,
        name: `User ${principal.toString().slice(-8)}`, // Use last 8 chars of principal as name
        ponziPoints: points,
        principal: principal.toString()
      }));
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 30000, // Refetch every 30 seconds for live updates
  });
}

export function useGetTopPonziPointsBurners() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['topPonziPointsBurners'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      const burners = await actor.getTopPonziPointsBurners();
      
      // Transform backend data to include user names
      return burners.map(([principal, pointsBurned], index) => ({
        rank: index + 1,
        name: `User ${principal.toString().slice(-8)}`, // Use last 8 chars of principal as name
        ponziPointsBurned: pointsBurned,
        principal: principal.toString()
      }));
    },
    enabled: !!actor && !actorFetching,
    refetchInterval: 30000, // Refetch every 30 seconds for live updates
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

// Ponzi Points calculation utilities
export function calculatePonziPoints(amount: number, plan: string, mode: 'simple' | 'compounding'): number {
  const basePoints = amount * 1000; // 1000 points per 1 ICP
  
  // Plan multipliers based on new structure
  let planMultiplier = 1.0;
  switch (plan) {
    case '21-day-simple':
      planMultiplier = 1.0; // 1x multiplier for simple
      break;
    case '15-day-compounding':
      planMultiplier = 2.0; // 2x multiplier for 15-day compounding
      break;
    case '30-day-compounding':
      planMultiplier = 3.0; // 3x multiplier for 30-day compounding
      break;
  }
  
  return basePoints * planMultiplier;
}

// Transaction History Query (Mock data until backend is implemented)
export function useGetTransactionHistory() {
  const { actor, isFetching: actorFetching } = useActor();

  return useQuery({
    queryKey: ['transactionHistory'],
    queryFn: async () => {
      if (!actor) throw new Error('Actor not available');
      
      // TODO: Replace with actual backend call when implemented
      // return actor.getTransactionHistory();
      
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
          exitTollFee: 1.75,
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
            exitTollFee: amount * 0.07,
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
    enabled: !!actor && !actorFetching,
  });
}
