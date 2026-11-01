"""Step 13 timing arithmetic (M14 devnet flow, 2026-11-01).

Integer-only copy of the program's rules, checked against the saved devnet
receipts before it is used:
  - Simple accrual: crates/economics/src/earnings.rs SimpleAccrual::accrue
    (principal * 11 * elapsed + remainder) / 8_640_000, remainder carried.
  - owed = accrued_gross - paid_gross: programs/musical_chairs/src/positions.rs simple_owed.
  - strict insolvency owed > pot, paid = min(owed, pot), toll floor(paid*bps/10_000):
    crates/economics/src/fees.rs payout / checked_bps.
  - Carried Interest tiers 1_200 / 750 / 300 bps at day 7 and day 14: fees.rs simple_toll_bps.
  - Compound15 owed floor(principal * ROI_15_Q18 / 1e18): positions.rs compound_owed.
Reads nothing, sends nothing.  Run: python3 -I step13_timing.py
"""
import json
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

DAY = 86_400
DEN = 8_640_000
Q18 = 10**18
ROI_15_Q18 = 4_473_565_759_257_037_859
ROI_30_Q18 = 12_267_678_469_131_246_220
PT = ZoneInfo("America/Los_Angeles")

# Saved devnet facts (receipts sha256 2d56a1bb..., steps-10-12 evidence sha256 4a375dca...).
GENESIS = 1_793_115_095 - 172_800          # ConfigVersion#1.activated_at minus the G delay
A1_OPEN, A1_PRINCIPAL = 1_793_115_467, 1_000_000_000
P4_OPEN, P5_OPEN, P_PRINCIPAL = 1_793_115_496, 1_793_115_515, 10_000_000
POT0 = 1_029_200_000                        # round 1 pot before any payout
F3_PAID, F3_TOLL, F3_NET = 456_785_185, 54_814_222, 401_970_963
T1_CREDIT, T1_SEED = 27_407_109, 27_407_113
POT_NOW = 572_414_815                       # after step 12; unchanged in the 2026-11-01 finalized snapshot
A1_LEFT = 62_000_000 - T1_CREDIT            # 34,592,891
CLOSE1_AT = 173_040 + 816_480               # plan offset of step 13 (O1 + 816,480)


def accrue(principal, last, remainder, accrued, now):
    whole, remainder = divmod(principal * 11 * (now - last) + remainder, DEN)
    return accrued + whole, remainder


def toll_bps(elapsed):
    return 1_200 if elapsed < 7 * DAY else 750 if elapsed < 14 * DAY else 300


def payout(owed, pot, bps):
    paid = min(owed, pot)
    toll = paid * bps // 10_000
    return {"paid_gross": paid, "toll": toll, "net": paid - toll, "insolvent": owed > pot}


def backer_half_single_row(toll):
    """35/45/20 slices of the Backer half, all to the only row (the rule ticket 1 followed)."""
    half = toll - toll // 2
    return sum(half * bps // 10_000 for bps in (3_500, 4_500, 2_000))


def stamp(unix):
    utc = datetime.fromtimestamp(unix, timezone.utc)
    return {"unix": unix, "utc": utc.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "pacific": utc.astimezone(PT).strftime("%Y-%m-%d %H:%M:%S %Z")}


out = {}

# 1. Self-checks against the saved receipts.
f3_e = 358_784
acc, rem = accrue(A1_PRINCIPAL, A1_OPEN, 0, 0, A1_OPEN + f3_e)
acc_pre, _ = accrue(A1_PRINCIPAL, A1_OPEN, 0, 0, A1_OPEN + f3_e - 1)
f3 = payout(acc, POT0, toll_bps(f3_e))
assert acc == F3_PAID and acc_pre == 456_783_912            # actual paid; the pre-state preview one second earlier
assert (f3["toll"], f3["net"]) == (F3_TOLL, F3_NET) and not f3["insolvent"]
assert backer_half_single_row(F3_TOLL) == T1_CREDIT and F3_TOLL - T1_CREDIT == T1_SEED
assert POT0 - F3_PAID == POT_NOW
out["self_checks"] = "F3 paid/toll/net, the step-10 preview, ticket 1 credit/seed and the pot all reproduce"

# 2. Earliest strict insolvency from A1's carried state (accrue from the F3 instant, remainder carried).
def owed_at(e):
    a, _ = accrue(A1_PRINCIPAL, A1_OPEN + f3_e, rem, acc, A1_OPEN + e)
    assert a == A1_PRINCIPAL * 11 * e // DEN                  # carrying the remainder == one-shot floor
    return a - F3_PAID

e = f3_e
while owed_at(e) <= POT_NOW:
    e += 1
e_star = e
assert all(owed_at(x) <= POT_NOW for x in (e_star - 1,)) and owed_at(e_star) > POT_NOW
# identity: owed - pot does not depend on F3 (both fell by F3_PAID)
assert (A1_PRINCIPAL * 11 * e_star // DEN) > POT0 >= (A1_PRINCIPAL * 11 * (e_star - 1) // DEN)
out["threshold"] = {
    "condition": "floor(1e9*11*e/8,640,000) - 456,785,185 > 572,414,815  <=>  floor(1e9*11*e/8,640,000) > 1,029,200,000",
    "e_star_s": e_star, "e_star_days": round(e_star / DAY, 6),
    "one_second_before": {**stamp(A1_OPEN + e_star - 1), "elapsed": e_star - 1,
                          "owed": owed_at(e_star - 1), "pot": POT_NOW, "owed_minus_pot": owed_at(e_star - 1) - POT_NOW,
                          "result": "solvent: a withdrawal would pay in full and NOT close"},
    "at_threshold": {**stamp(A1_OPEN + e_star), "elapsed": e_star,
                     "owed": owed_at(e_star), "pot": POT_NOW, "owed_minus_pot": owed_at(e_star) - POT_NOW,
                     "result": "strictly insolvent: pot-capped payout closes round 1"},
    "toll_bps_at_threshold": toll_bps(e_star),
}

# 3. The plan's step 13 instant and the chosen margin.
plan13 = GENESIS + CLOSE1_AT
plan_e = plan13 - A1_OPEN
out["plan_step13"] = {**stamp(plan13), "elapsed": plan_e, "owed_minus_pot": owed_at(plan_e) - POT_NOW,
                      "margin_s": plan13 - (A1_OPEN + e_star)}

# 4. The close outcome is the same anywhere in [e*, 14 d): pot-capped, 7.5 % tier.
close = payout(owed_at(e_star), POT_NOW, toll_bps(e_star))
for x in (plan_e, 14 * DAY - 1):
    assert payout(owed_at(x), POT_NOW, toll_bps(x)) == close
assert toll_bps(14 * DAY) != 750 and toll_bps(7 * DAY - 1) != 750
t2_credit = backer_half_single_row(close["toll"])
t2_seed = close["toll"] - t2_credit
a1_left = A1_LEFT - t2_credit
a1_net = F3_NET + close["net"]
seed_r2 = T1_SEED + t2_seed
c15_owed = 200_000_000 * ROI_15_Q18 // Q18
c15 = payout(c15_owed, seed_r2 + 920_000_000 * 96 // 100 + 100_000_000, 900)
c15_half = c15["toll"] - c15["toll"] // 2
c15_senior = c15_half * 3_500 // 10_000
pot_c30 = seed_r2 + 920_000_000 * 96 // 100 + 100_000_000 - c15["paid_gross"]
c30_owed = 600_000_000 * ROI_30_Q18 // Q18
out["close1_same_for_any_close_instant"] = {
    "valid_window_elapsed_s": [e_star, 14 * DAY - 1],
    "valid_window_end": stamp(A1_OPEN + 14 * DAY - 1),
    "close_payout": close, "ticket2_a1_credit": t2_credit, "ticket2_seed": t2_seed,
    "a1_entitlement_left_after_round1": a1_left,
    "a1_round1_net": a1_net, "a1_loser_by": A1_PRINCIPAL - a1_net,
    "projections_not_chain_facts": {
        "round2_seed_carried": seed_r2,
        "c15_owed": c15_owed, "c15_pot": seed_r2 + 883_200_000 + 100_000_000, "c15_solvent": not c15["insolvent"],
        "c15_senior_slice": c15_senior, "f6_holds": a1_left <= c15_senior, "f6_margin": c15_senior - a1_left,
        "c30_pot": pot_c30, "c30_owed": c30_owed, "c30_strictly_insolvent": c30_owed > pot_c30,
    },
}

# 5. Separate option only (extra transactions, changed amounts): P4 and P5 withdraw everything
#    just before step 13.  Each withdrawal starts a ticket that must commit before the next payout.
def p_owed(open_at, t):
    return P_PRINCIPAL * 11 * (t - open_at) // DEN

e = 780_000
while not (A1_PRINCIPAL * 11 * e // DEN - F3_PAID
           > POT_NOW - p_owed(P4_OPEN, A1_OPEN + e) - p_owed(P5_OPEN, A1_OPEN + e)):
    e += 1
p4 = payout(p_owed(P4_OPEN, A1_OPEN + e), POT_NOW, toll_bps(e))
out["option_extra_candidate_withdrawals"] = {
    "earliest_close_if_both_withdraw_at_that_instant": {**stamp(A1_OPEN + e), "elapsed": e},
    "gain_vs_threshold_s": e_star - e,
    "p4_net_payout": p4["net"], "p4_loss_becomes": P_PRINCIPAL - p4["net"],
    "winner_series_b_entitlement_becomes": (P_PRINCIPAL - p4["net"]) * 11_600 // 10_000,
    "winner_series_b_entitlement_plan": P_PRINCIPAL * 11_600 // 10_000,
    "needs": "2 extra withdrawals + 2 extra keeper tickets, new runner steps, changed Winner B amounts and budgets",
}
out["new_deposits_cannot_help"] = {
    "reason": "a deposit adds 96 % to the pot at once and its own accrual (11 %/day) can only take it back after 0.96/0.11 days",
    "days": round(0.96 / 0.11, 4),
}

# 6. Sensitivity: a FOREIGN round 1 inflow (the program is public on devnet) moves the threshold later.
def threshold_with_extra_pot(extra):
    x = e_star
    while not (A1_PRINCIPAL * 11 * x // DEN > POT0 + extra):
        x += 1
    return x - e_star

out["foreign_inflow_delays_threshold_s"] = {
    "min_game_deposit_0.01_sol_(pot +9,600,000)": threshold_with_extra_pot(9_600_000),
    "min_series_a_0.05_sol_(pot +50,000,000)": threshold_with_extra_pot(50_000_000),
}

# 7. Remaining plan instants under the accepted schedule (genesis + offset) and the program's own bounds.
R2O = CLOSE1_AT + 180
P1_OPEN = R2O + 60
C15_AT, C30_AT = P1_OPEN + 1_296_060, R2O + 2_592_060
out["accepted_schedule"] = {
    "step13": stamp(GENESIS + CLOSE1_AT),
    "step17_19_deadline_if_closed_at_plan": stamp(GENESIS + CLOSE1_AT + 600),
    "step21_reopen": stamp(GENESIS + R2O),
    "step22_p1_open": stamp(GENESIS + P1_OPEN),
    "step28_c15_plan": stamp(GENESIS + C15_AT),
    "step28_c15_maturity_if_22_on_time": stamp(GENESIS + P1_OPEN + 1_296_000),
    "step32_c30_plan": stamp(GENESIS + C30_AT),
    "step32_c30_maturity_if_21_on_time": stamp(GENESIS + R2O + 2_592_000),
    "step37_runner_earliest": stamp(GENESIS + C30_AT + 600),
    "step39_last": stamp(GENESIS + C30_AT + 611),
}
print(json.dumps(out, indent=2))
