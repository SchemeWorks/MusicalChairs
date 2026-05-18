# Trollbox — Design Spec

**Date:** 2026-05-18
**Status:** Draft, pre-implementation
**Author:** Charles + Claude (brainstormed in worktree `suspicious-taussig-f52cc7`)

## Goal

Add a persistent in-app chat ("Trollbox") that floats on every page, doubles as a live activity feed for the satire's most narratively rich events, and leans hard into the MLM/VC voice already established in the product.

The chat survives tab navigation, opens/closes via a single floating icon in the lower-right, and remembers per-user open/close preference. The same widget renders user messages, shenanigan-cast callouts, signups, MLM rank-ups, Musical Chairs round results, a pinned "Management" announcement, and dry one-liners from an automated compliance bot named Reginald.

## Scope

In scope (v1):

- Floating bottom-right icon + collapsible chat panel mounted globally above the tab router.
- Plain-text user messages (280 chars max) from authenticated principals; logged-out users see everything but can't post.
- A unified heterogeneous feed merging user messages and system events (spell casts, signups, rank-ups, round results, Reginald lines).
- A pinned "Management" announcement banner at the top of the chat panel, editable by the admin (Charles).
- Soft-delete + mute + pin admin controls in the existing `<ShenanigansAdminPanel />`.
- Free emoji reactions (fixed allowlist).
- PP-cost "karma" reactions (minimum 10 PP, no max, burned via existing PP ledger path).
- Client-side @-mentions with soft chime when the current user is mentioned.
- Client-side block list (per-browser, localStorage).
- Mobile full-screen takeover with close button.
- Unread badge on the floating icon when closed.

Explicitly out of scope (deferred):

- Money-flow events in chat (deposits, cash-outs).
- Threaded replies in the UI (the `replyTo` field is stored for future use; v1 renders flat).
- Editing messages.
- Charles taking a cut of karma PP burns (v1 burns 100%).
- Editable Reginald flavor pool via admin UI (hardcode initial pool, edit by code change).
- Server-side block list.
- Rich text / image attachments.

## Naming

User-facing widget name: **Trollbox**. Internal code identifier: `trollbox` / `Trollbox`. The pinned banner is referred to as "the pin" in code and "Management Announcement" in UI copy. Reginald is the automated bot character (consistent with the Insurance feature in the shenanigan backlog).

## Architecture overview

One unified bounded ring buffer (`chatItems`, capped at 500) inside the existing `shenanigans` canister. Every writer — user posts, the cast handler, the signup observer, the rank-up detector, the round observer, Reginald, Charles' pin edits — appends to it.

The shenanigans canister is the natural host because it already (a) holds the spell history buffer with the same 500-record pattern, (b) is polled every 3 seconds by the frontend, (c) observes signups and round resolution for the mint loop, and (d) tracks `ReferralStats` needed for rank-up detection.

Existing methods (`getRecentShenanigans`, `getReferralStats`, observer loops, etc.) are untouched. The trollbox is purely additive.

Rejected alternative: separate streams merged client-side. The client merge logic and triple-poll cost outweigh the cheap duplication of spell records into chat items.

## Data model (shenanigans canister)

```motoko
public type ChatItemKind = {
    #userMessage : { body : Text; replyTo : ?Nat };
    #spellCast : { castId : Nat };
    #signup : { newUser : Principal };
    #rankUp : { user : Principal; newRank : Text };
    #roundResult : { gameId : Nat; winner : Principal; pot : Nat };
    #reginald : { line : Text; triggerKind : Text };
    #pinUpdate : { body : Text };
};

public type Reaction = {
    emoji : Text;
    reactors : [Principal];     // dedup; client may hide blocked reactors
    karmaPpBurned : Nat;        // aggregate; 0 for free reactions
};

public type ChatItem = {
    id : Nat;
    author : Principal;          // for system items: shenanigans canister principal; for #pinUpdate / #reginald (manual): admin principal
    timestamp : Int;             // ns since epoch
    kind : ChatItemKind;
    reactions : [Reaction];
    deleted : Bool;              // soft-delete; preserves ordering; renders as "[removed by Management]"
};

stable var chatItems : [ChatItem] = [];       // bounded to last 500, newest at head
stable var nextChatItemId : Nat = 0;
stable var mutedUntil : [(Principal, Int)] = []; // sorted, expired entries pruned lazily
stable var currentPinId : ?Nat = null;
stable var previousRankByUser : [(Principal, Text)] = []; // for rank-up detection
stable var lastChatPostByUser : [(Principal, Int)] = [];  // for rate limit
```

Persistence: this state survives upgrades via the same stable-var pattern the canister already uses for spell history. The 500-cap means oldest items drop off the end; this is acceptable for a trollbox.

## Backend API

All methods added to the `shenanigans` canister.

### Reads (public, no auth required)

- `getRecentChatItems(limit : Nat) : async [ChatItem]` — newest-first, capped server-side at 100 per call. Default frontend usage: `limit = 100`.
- `getCurrentPin() : async ?ChatItem` — convenience for the banner.
- `isMuted(user : Principal) : async ?Int` — returns mute expiry timestamp if muted; helps the client suppress the composer.

### Writes (authenticated)

- `postChatMessage(body : Text, replyTo : ?Nat) : async { #Ok : Nat; #Err : Text }`
  - Rejects anonymous principal.
  - Validation: `1 <= length(body) <= 280`. Strip control characters (keep tab/newline). No HTML — client renders plain text and linkifies URLs.
  - Rate limit: 1 post per 3 seconds per principal, AND 15 posts per 5-minute window per principal. Returns `#Err "Slow down."` on violation.
  - Mute check: returns `#Err "You are muted until {ts}."` if `mutedUntil[caller] > now`.
- `addReaction(itemId : Nat, emoji : Text) : async { #Ok; #Err : Text }`
  - Free reaction. Emoji must be in the allowlist: `👍 😂 🔥 💀 🎯 🙏`.
  - Idempotent per `(caller, itemId, emoji)`.
- `removeReaction(itemId : Nat, emoji : Text) : async ()` — removes own reaction; no-op if not present.
- `addKarmaReaction(itemId : Nat, emoji : Text, ppToBurn : Nat) : async { #Ok; #Err : Text }`
  - Karma reaction. Emoji allowlist: `👍 😂 🔥 💀 🎯 🙏 💰 🚀`.
  - `ppToBurn >= 10` (whole PP units, scaled per existing PP ledger conventions).
  - Burn happens before the reaction is appended. If the PP-ledger burn returns `#Err`, the reaction is not added and the error is propagated.
  - On success ≥100 PP, also append a `#reginald` chat item narrating the reaction.

### Admin-only (Charles)

- `adminDeleteChatItem(itemId : Nat) : async ()` — sets `deleted = true`.
- `adminMuteUser(user : Principal, durationSeconds : Nat) : async ()` — sets `mutedUntil[user] = now + duration`.
- `adminUnmute(user : Principal) : async ()` — removes mute entry.
- `adminSetPin(body : Text) : async Nat` — appends a new `#pinUpdate` chat item with the new body and updates `currentPinId`. Empty body clears the pin (sets `currentPinId = null`).
- `adminPostAsReginald(line : Text) : async Nat` — appends `#reginald { line; triggerKind = "manual" }`.

All admin methods use the existing `assertCallerIsAdmin` pattern in `shenanigans/main.mo`.

### Internal event emitters (no public API)

- **Signup**: existing signup-handling path (in `mintWithEffects` neighborhood) appends `#signup` when a new principal is added to `signups`.
- **Spell cast**: existing `castShenanigan` handler appends `#spellCast { castId = record.id }` on success/fail/backfire (one item per cast).
- **Rank-up**: after `distributeDeductiveCascade` runs, compute current rank for the recipient using the rank thresholds (below). Compare to `previousRankByUser[user]`. If higher tier, append `#rankUp` and update the stored rank.
- **Round result**: existing round-resolution observer (currently emits mints) appends `#roundResult { gameId; winner; pot }` when a round ends.
- **Reginald auto-triggers**:
  - Backfired `#spellCast` → 25% chance to append a `#reginald` line tagged `"spellBackfire"`.
  - `#rankUp` to Affiliate or higher → 100% chance, tagged `"rankUp"`.
  - `#roundResult` → 15% chance, tagged `"roundResult"`.
  - `postChatMessage` body containing any of `guaranteed`, `no risk`, `100%`, `pump` (case-insensitive) → 100% chance, tagged `"buzzword"`.
  - Karma reaction ≥100 PP → 100% chance, tagged `"karma"`.

Reginald flavor pool is a `[Text]` per trigger kind, hardcoded in v1. Selection is round-robin or pseudo-random (use existing randomness already in the cast handler).

## MLM rank tiers

Thresholds for `#rankUp` detection (compares against `ReferralStats`):

| Tier | Threshold |
|---|---|
| Cold Lead | 0 referrals (default) |
| Affiliate | ≥1 direct referral |
| Junior Partner | ≥5 direct referrals OR ≥10 total downline |
| Senior Advisor | ≥15 direct OR ≥40 total downline |
| Regional Director | ≥30 direct OR ≥100 total downline |
| Diamond Director | ≥60 direct OR ≥250 total downline |
| Triple-Diamond Founder's Circle | ≥100 direct OR ≥500 total downline |

`#rankUp` only emits on upward crossings. No `#rankDown` (it would never actually happen given the cumulative referral model).

## Frontend architecture

### Mount point

A new `<Trollbox />` component is mounted in `App.tsx` at the root of the authenticated/unauthenticated app, outside the tab router. Rendering nothing-visible by default except the floating bottom-right icon. It survives tab switches because it lives above tab content.

### Component tree

```
App.tsx
└── <Trollbox />
    ├── <TrollboxFab />                  // closed-state floating button + unread badge
    └── <TrollboxPanel />                // open-state: desktop docked card / mobile full-screen
        ├── <PinnedBanner />             // currentPin from getCurrentPin
        ├── <ChatStream />               // virtualised list of <ChatItemRow />
        │   └── <ChatItemRow />          // dispatches on kind
        ├── <ReactionPicker />           // shown on long-press / hover
        └── <Composer />                 // textarea + send; hidden if logged out or muted
```

### Hooks (added to `useQueries.ts`)

- `useRecentChatItems()` — `useQuery`, `refetchInterval: 3000` (matches existing `useGetRecentShenanigans`).
- `useCurrentPin()` — `useQuery`, `refetchInterval: 15000` (pin changes are rare).
- `useIsMuted(principal)` — `useQuery`, `refetchInterval: 60000`.
- `usePostChatMessage()` — mutation, invalidates chat items on success.
- `useAddReaction()` / `useRemoveReaction()` / `useKarmaReact()` — mutations, optimistic updates allowed for free reactions; karma reactions wait for server confirmation (because PP balance must update).

### Local state & persistence (localStorage)

- `trollbox.open` — boolean, last open/close state.
- `trollbox.lastSeenId` — highest chat item id seen by this user; drives the unread badge.
- `trollbox.blocked` — array of principal-text strings to filter out.
- `trollbox.chimeMuted` — boolean, in-header mute toggle for the @-mention chime.

### Rendering rules per `ChatItemKind`

- `#userMessage` — display name (resolved from `getUserProfile` with golden-name override taking precedence), avatar initial chip, body, timestamp, reactions row. `@name` tokens render as chips. If the current user is mentioned, the row gets a left accent stripe.
- `#spellCast` — joins to `ShenaniganRecord` from the existing `getRecentShenanigans` cache. Uses existing flavor text + icon + aura color. Backfire and golden-name effects use the existing glow classes from [Shenanigans.tsx](frontend/src/components/Shenanigans.tsx). Fallback string when join misses (record fell out of spell-history buffer): `"{user} cast a spell."`
- `#signup` — `"🆕 {name} just signed the dotted line."`
- `#rankUp` — `"📈 {name} promoted to {newRank}."`
- `#roundResult` — `"🎰 Round #{gameId} — {winner} took the chair. Pot: {pot} ICP."`
- `#reginald` — italic, "Reginald" name chip with small bot avatar, dry one-liner.
- `#pinUpdate` — NOT rendered inline. Consumed only by `<PinnedBanner />` (via `getCurrentPin`).
- Items with `deleted = true` render as `"[removed by Management]"`.

### Auth & moderation states (composer)

- Logged out: composer hidden. Footer reads "Sign in to join the conversation."
- Logged in + muted: composer replaced by "You've been muted by the Management until {expiry}."
- Logged in + posting allowed: full composer with 280-char counter and send button.

### Responsive layout

- Desktop (≥768px): docked card, 380px wide × 560px tall, bottom-right, 16px from edges, rounded, dark with the existing aesthetic.
- Mobile (<768px): full-screen overlay. Top header with drag-handle indicator + close ✕. Composer pinned to bottom; chat list scrolls between header and composer.

### @-mention chime

- Single `Audio` element preloaded with a short blip.
- Plays only when: user is authenticated AND panel is closed AND a new item's body contains `@<currentUser.name>` AND `trollbox.chimeMuted !== true`.
- Debounced to once per 5 seconds.

### Block list

- Client-side only in v1. Stored in `localStorage('trollbox.blocked')`.
- `<ChatItemRow />` filters out user-message items whose author principal is in the block list.
- Reactions from blocked principals are also hidden; karma totals still display the aggregate count.
- Long-press / right-click on a user-message row exposes "Block this user".

## Admin UI

A new collapsible "Trollbox" section in the existing `<ShenanigansAdminPanel />`:

- **Pin editor** — textarea (≤500 chars) + Save / Clear buttons.
- **Mute list** — table: principal, expiry timestamp, Unmute button.
- **Mute user** — input for principal text + duration dropdown (1h / 24h / 7d / forever).
- **Post as Reginald** — composer (≤280 chars).
- **Inline delete** — long-press on any chat item in the main chat surfaces a 🗑️ affordance when the caller is admin (driven by the existing `isCharles` check).

## Edge cases addressed by design

- **Renames** — display name resolves per render from `getUserProfile` (with golden-name override taking precedence), so a rename retroactively updates all past messages. On-brand.
- **Spell-cast join miss** — chat item stores `castId`; if the underlying `ShenaniganRecord` has fallen out of the 500-record spell history buffer, the chat row falls back to a generic spell-cast string.
- **Mention chime spam** — debounced to once per 5 seconds, respects `trollbox.chimeMuted`.
- **Block list bypass via reactions** — blocked user's reactions are hidden; aggregate karma totals still display.
- **Pin update churn** — `#pinUpdate` items live in the buffer but never render in the stream, so Charles can edit the pin without spamming.
- **Anonymous principal** — explicitly rejected in all write endpoints (`postChatMessage`, `addReaction`, `addKarmaReaction`). Reads are open.
- **PP race on karma** — burn happens before reaction is appended; failed burn → no reaction added; error propagates to caller.
- **Bounded buffer eviction** — items drop off the end (oldest first) when buffer exceeds 500. The `currentPinId` may point to an evicted item; `getCurrentPin` returns `null` in that case (acceptable; Charles can re-pin).
- **Rate-limit clock** — `lastChatPostByUser` updated on accepted post only, not on rejected posts. Prevents an attacker from preventing legitimate posts by spamming rejected ones.

## Testing strategy

### Backend (Motoko)

Unit-style tests in the existing shenanigans test harness (or PocketIC if available) for:

- 280-char length validation (boundary: 0, 1, 280, 281).
- Control-character stripping.
- Rate limit: post twice within 3s → second rejected; 16 posts in 5min → 16th rejected.
- Mute enforcement: muted user's post returns `#Err`; mute expiry honors timestamp.
- Anonymous principal rejected on all three write endpoints.
- Karma reaction with insufficient PP → reaction not added.
- Soft-delete: deleted item still occupies its position in the buffer; `deleted = true`.
- Bounded buffer: appending 501st item evicts the oldest.
- Rank-up detection: appending a referral that crosses a threshold appends `#rankUp` once; subsequent referrals don't re-emit until next threshold.
- Reginald buzzword detection: case-insensitive match on the four keywords.

### Frontend

Manual verification per the preview workflow:

- Send a message → appears in own client immediately, in another browser within 3s.
- Floating icon toggles panel open/close; preference persists across reload.
- Unread badge appears when closed and new items arrive; clears on open.
- @-mention chime plays for the mentioned user; doesn't play when panel is open or chime is muted.
- Mobile full-screen takeover works at <768px viewport.
- Pinned banner shows latest pin; admin pin update reflects within 15s.
- Free reaction tap toggles correctly; karma reaction deducts PP and triggers Reginald line at ≥100 PP.
- Blocked user's messages and reactions are hidden after long-press → Block.
- Logged-out user sees the panel but no composer.
- Muted user sees the muted message in place of the composer.
- Admin can delete any message via long-press; deleted items render as `"[removed by Management]"`.

## Open questions for future iterations

- Should Charles take a cut of karma PP burns (e.g., 30%)? Currently 100% burned. On-brand to skim, but adds complexity.
- Should the Reginald flavor pool be editable via admin UI? Currently hardcoded.
- Should we surface a server-side block list so blocks persist across browsers? Currently client-only.
- Should `replyTo` render as quote-cards in v2? The field is already stored.
