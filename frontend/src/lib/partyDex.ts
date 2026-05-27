/**
 * PartyDEX integration — PP/ICP market canister on Internet Computer.
 *
 * Docs: https://partyhats.xyz/docs/agents/canister/integration
 *
 * Flow for a single-shot market buy from a user's wallet:
 *   1. (one-time or per-buy) icrc2_approve on ICP ledger → spender = PartyDEX canister
 *   2. quote_trade(#buy, input_amount, ?limit_tick, ?slippage_bps) — free query
 *   3. non_atomic_trade({ book_orders, pool_swaps, min_output, allow_partial })
 *      PartyDEX pulls ICP via transfer_from, executes the route across book + AMM,
 *      and pushes PP back to the user's wallet in the same call.
 *
 * No deposit/withdraw needed in the happy path. If the outbound transfer leg fails,
 * tokens sit in the user's "trading balance" inside PartyDEX and are recoverable
 * via withdraw — surfaced as a recovery flow elsewhere if/when it ever happens.
 */

import { Actor, HttpAgent, ActorSubclass } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';

// PartyDEX spot canister for the PP/ICP pair.
export const PARTYDEX_PP_ICP_CANISTER_ID = 'l6o5h-dqaaa-aaaai-axwpq-cai';

// "Any price" sentinel — passed as limit_tick to quote_trade for a pure market order.
// Per PartyDEX docs: 887272 is the upper tick bound; using it disables price limiting.
export const LIMIT_TICK_NONE = 887272;

// Default slippage in basis points (50 bps = 0.5%). Conservative for impulse buys.
export const DEFAULT_SLIPPAGE_BPS = 50;

// Max nat for "Fast Buy" — one-time approval covering all future buys.
export const MAX_NAT_E8S = 340_282_366_920_938_463_463_374_607_431_768_211_455n;

const HOST = 'https://icp0.io';

// Candid types — manually transcribed from the canister's .did. Only the methods
// we actually call are exposed.
type Side = { buy: null } | { sell: null };
type TimeInForce = { gtc: null } | { ioc: null } | { fok: null };

interface BookOrderSpec {
  side: Side;
  input_amount: bigint;
  limit_tick: number; // Tick = int32
  time_in_force: TimeInForce;
}

interface PoolSwapSpec {
  side: Side;
  input_amount: bigint;
  limit_tick: number;
  fee_pips: number; // nat32
}

interface VenueBreakdown {
  venue_id: { book: null } | { pool: number };
  input_amount: bigint;
  output_amount: bigint;
  fee_amount: bigint;
}

export interface QuoteResult {
  input_amount: bigint;
  output_amount: bigint;
  pool_swaps: PoolSwapSpec[];
  book_orders: BookOrderSpec[];
  total_fees: bigint;
  effective_tick: number;
  reference_tick: number;
  venue_breakdown: VenueBreakdown[];
}

export interface ApiError {
  category:
    | { validation: null }
    | { authorization: null }
    | { state: null }
    | { resource: null }
    | { rate_limit: null }
    | { external: null }
    | { admin: null }
    | { other: null };
  code: string;
  message: string;
  metadata: [] | [Array<[string, string]>];
}

export type QuoteTradeResult = { ok: QuoteResult } | { err: ApiError };

type TransferLegOutcome =
  | { not_attempted: null }
  | { failed: ApiError }
  | { transferred: { amount: bigint; block_index: bigint } };

interface TransferLegs {
  base: TransferLegOutcome;
  quote: TransferLegOutcome;
}

export interface NonAtomicTradeOk {
  versions: unknown;
  order_results: Array<{ index: number; result: unknown }>;
  swap_results: Array<{ index: number; result: unknown }>;
  output: TransferLegs;
  refund: TransferLegs;
}

export type NonAtomicTradeResult = { ok: NonAtomicTradeOk } | { err: ApiError };

// Slim RoutingState — only the fields we actually use. quote_usd_rate_e12 is the
// USD value of one natural quote token (ICP), encoded × 10^12. We use it to enforce
// PartyDEX's minimum trade size (currently ~$1) before the user clicks BUY.
export interface RoutingStateLite {
  quote_usd_rate_e12: bigint;
  current_price_usd_e12: bigint;
}

export interface PartyDexService {
  quote_trade: (
    side: Side,
    input_amount: bigint,
    limit_tick: [] | [number],
    slippage_bps: [] | [number],
  ) => Promise<QuoteTradeResult>;
  non_atomic_trade: (args: {
    book_orders: BookOrderSpec[];
    pool_swaps: PoolSwapSpec[];
    min_output: [] | [bigint];
    allow_partial: boolean;
  }) => Promise<NonAtomicTradeResult>;
  withdraw: (
    token: { base: null } | { quote: null },
    amount: bigint,
  ) => Promise<unknown>;
  get_routing_state: () => Promise<RoutingStateLite>;
}

// IDL factory — manually written to mirror the .did service definition.
export const partyDexIdl = ({ IDL }: { IDL: any }) => {
  const Side = IDL.Variant({ buy: IDL.Null, sell: IDL.Null });
  const TimeInForce = IDL.Variant({ gtc: IDL.Null, ioc: IDL.Null, fok: IDL.Null });
  const Tick = IDL.Int32;

  const BookOrderSpec = IDL.Record({
    side: Side,
    input_amount: IDL.Nat,
    limit_tick: Tick,
    time_in_force: TimeInForce,
  });

  const PoolSwapSpec = IDL.Record({
    side: Side,
    input_amount: IDL.Nat,
    limit_tick: Tick,
    fee_pips: IDL.Nat32,
  });

  const ApiError = IDL.Record({
    category: IDL.Variant({
      validation: IDL.Null,
      authorization: IDL.Null,
      state: IDL.Null,
      resource: IDL.Null,
      rate_limit: IDL.Null,
      external: IDL.Null,
      admin: IDL.Null,
      other: IDL.Null,
    }),
    code: IDL.Text,
    message: IDL.Text,
    metadata: IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, IDL.Text))),
  });

  const VenueBreakdown = IDL.Record({
    venue_id: IDL.Variant({ book: IDL.Null, pool: IDL.Nat32 }),
    input_amount: IDL.Nat,
    output_amount: IDL.Nat,
    fee_amount: IDL.Nat,
  });

  const QuoteResult = IDL.Record({
    input_amount: IDL.Nat,
    output_amount: IDL.Nat,
    pool_swaps: IDL.Vec(PoolSwapSpec),
    book_orders: IDL.Vec(BookOrderSpec),
    total_fees: IDL.Nat,
    effective_tick: Tick,
    reference_tick: Tick,
    venue_breakdown: IDL.Vec(VenueBreakdown),
  });

  const QuoteTradeResult = IDL.Variant({ ok: QuoteResult, err: ApiError });

  const SystemGuard = IDL.Record({
    system_state: IDL.Variant({ normal: IDL.Null, degraded: IDL.Null, halted: IDL.Null }),
    global_backpressure: IDL.Bool,
    user_calls_remaining: IDL.Int,
  });

  const PollVersions = IDL.Record({
    platform: IDL.Nat,
    orderbook: IDL.Nat,
    candle: IDL.Nat,
    user: IDL.Nat,
    guard: SystemGuard,
    available_base: IDL.Nat,
    available_quote: IDL.Nat,
  });

  const TransferLegSuccess = IDL.Record({ amount: IDL.Nat, block_index: IDL.Nat });
  const TransferLegOutcome = IDL.Variant({
    not_attempted: IDL.Null,
    failed: ApiError,
    transferred: TransferLegSuccess,
  });
  const TransferLegs = IDL.Record({ base: TransferLegOutcome, quote: TransferLegOutcome });

  const OrderResultItem = IDL.Record({
    index: IDL.Nat32,
    result: IDL.Variant({
      ok: IDL.Record({
        order_id: IDL.Nat64,
        settlement: IDL.Variant({
          filled: IDL.Null,
          partial: IDL.Null,
          resting: IDL.Null,
          cancelled: IDL.Null,
          fok_rejected: IDL.Null,
        }),
        input_amount: IDL.Nat,
        output_amount: IDL.Nat,
        remaining_input: IDL.Nat,
        fee: IDL.Int,
      }),
      err: ApiError,
    }),
  });

  const SwapResultItem = IDL.Record({
    index: IDL.Nat32,
    result: IDL.Variant({
      ok: IDL.Record({ input_amount: IDL.Nat, output_amount: IDL.Nat, fee: IDL.Int }),
      err: ApiError,
    }),
  });

  const NonAtomicTradeArgs = IDL.Record({
    book_orders: IDL.Vec(BookOrderSpec),
    pool_swaps: IDL.Vec(PoolSwapSpec),
    min_output: IDL.Opt(IDL.Nat),
    allow_partial: IDL.Bool,
  });

  const NonAtomicTradeResult = IDL.Variant({
    ok: IDL.Record({
      versions: PollVersions,
      order_results: IDL.Vec(OrderResultItem),
      swap_results: IDL.Vec(SwapResultItem),
      output: TransferLegs,
      refund: TransferLegs,
    }),
    err: ApiError,
  });

  const WithdrawResult = IDL.Variant({
    ok: IDL.Record({ versions: PollVersions, amount: IDL.Nat, block_index: IDL.Nat }),
    err: ApiError,
  });

  // Loose decoder for get_routing_state: declare only the two USD rate fields we
  // actually consume. Candid record decoding accepts unknown extra fields, so
  // skipping `book`, `pools`, etc. keeps the IDL small without breaking parsing.
  // (Actually it's the OPPOSITE — Candid decoding REQUIRES all declared fields,
  // but extra wire fields ARE allowed. So we declare ALL fields here.)
  const TokenMetadata = IDL.Record({ ledger: IDL.Principal, decimals: IDL.Nat8, fee: IDL.Nat });
  const TickLiquidityData = IDL.Record({ liquidity_gross: IDL.Nat, liquidity_net: IDL.Int, tick: Tick });
  const BookLevelRaw = IDL.Record({ tick: Tick, total: IDL.Nat });
  const BookLevelsRaw = IDL.Record({ asks: IDL.Vec(BookLevelRaw), bids: IDL.Vec(BookLevelRaw) });
  const RoutingPoolView = IDL.Record({
    base_reserve: IDL.Nat, fee_pips: IDL.Nat32, initialized_ticks: IDL.Vec(TickLiquidityData),
    liquidity: IDL.Nat, quote_reserve: IDL.Nat, sqrt_price_x96: IDL.Nat,
    tick: Tick, tick_spacing: IDL.Nat,
  });
  const SystemStateVariant = IDL.Variant({ normal: IDL.Null, degraded: IDL.Null, halted: IDL.Null });
  const RoutingState = IDL.Record({
    base: TokenMetadata, quote: TokenMetadata, book: BookLevelsRaw,
    current_price_usd_e12: IDL.Nat,
    last_book_tick: IDL.Opt(Tick),
    last_trade_sqrt_price_x96: IDL.Opt(IDL.Nat),
    last_trade_tick: IDL.Opt(Tick),
    maker_fee_pips: IDL.Nat32,
    pools: IDL.Vec(RoutingPoolView),
    quote_usd_rate_e12: IDL.Nat,
    reference_ask_tick: IDL.Opt(Tick),
    reference_bid_tick: IDL.Opt(Tick),
    reference_tick: IDL.Opt(Tick),
    system_state: SystemStateVariant,
    taker_fee_pips: IDL.Nat32,
  });

  return IDL.Service({
    quote_trade: IDL.Func(
      [Side, IDL.Nat, IDL.Opt(Tick), IDL.Opt(IDL.Nat32)],
      [QuoteTradeResult],
      ['query'],
    ),
    non_atomic_trade: IDL.Func([NonAtomicTradeArgs], [NonAtomicTradeResult], []),
    withdraw: IDL.Func([IDL.Variant({ base: IDL.Null, quote: IDL.Null }), IDL.Nat], [WithdrawResult], []),
    get_routing_state: IDL.Func([], [RoutingState], ['query']),
  });
};

// Cached anonymous actor for read-only queries (quote_trade is `query`, so this is
// safe and fast — no identity, no signing).
let cachedReadActor: ActorSubclass<PartyDexService> | null = null;

export function getPartyDexReadActor(): ActorSubclass<PartyDexService> {
  if (cachedReadActor) return cachedReadActor;
  const agent = new HttpAgent({ host: HOST });
  cachedReadActor = Actor.createActor<PartyDexService>(partyDexIdl, {
    agent,
    canisterId: PARTYDEX_PP_ICP_CANISTER_ID,
  });
  return cachedReadActor;
}

// Build an authed actor on demand for non_atomic_trade. Mirrors useActor — Plug
// gets its bundled agent, II/others get a fresh HttpAgent with their identity.
export async function createPartyDexAuthedActor(args: {
  walletType: string | null;
  identity: any | null;
}): Promise<ActorSubclass<PartyDexService>> {
  if (args.walletType === 'plug') {
    if (!window.ic?.plug?.agent) throw new Error('Plug agent not initialized');
    return Actor.createActor<PartyDexService>(partyDexIdl, {
      agent: window.ic.plug.agent as any,
      canisterId: PARTYDEX_PP_ICP_CANISTER_ID,
    });
  }
  if (!args.identity) throw new Error('No identity for PartyDEX authed call');
  const agent = new HttpAgent({ host: HOST, identity: args.identity });
  return Actor.createActor<PartyDexService>(partyDexIdl, {
    agent,
    canisterId: PARTYDEX_PP_ICP_CANISTER_ID,
  });
}

// Convenience: spender Account for PartyDEX (used in icrc2_approve calls).
export function getPartyDexSpenderPrincipal(): Principal {
  return Principal.fromText(PARTYDEX_PP_ICP_CANISTER_ID);
}
