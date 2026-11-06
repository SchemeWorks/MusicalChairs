"""Close 1 margin removal (M14 devnet flow, 2026-11-01): the revised schedule, recomputed independently.

Integer-only copy of the program's rules (the same ones step13_timing.py cites), checked against the saved
devnet receipts first:
  - Simple accrual floor(principal * 11 * elapsed / 8_640_000), remainder carried (equals the one-shot floor).
  - strict insolvency owed > pot; paid = min(owed, pot); toll floor(paid * bps / 10_000); 7.5 % tier from day 7 to 14.
  - Backer half of a toll = toll - toll // 2, split 35/45/20 to the only Series A row in round 1.
Reads nothing, sends nothing.  Run: python3 -I close1_shift.py
"""
import json
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

DAY, DEN = 86_400, 8_640_000
PT = ZoneInfo("America/Los_Angeles")

# Devnet facts (receipts sha256 2d56a1bb..., the step 13 assessment's finalized status).
GENESIS = 1_793_115_095 - 172_800            # ConfigVersion#1.activated_at minus the G delay
T0, O1 = 172_800, 172_800 + 240              # plan offsets
A1_OPEN, A1_PRINCIPAL = 1_793_115_467, 1_000_000_000
POT0, F3_PAID, F3_TOLL, T1_CREDIT, T1_SEED = 1_029_200_000, 456_785_185, 54_814_222, 27_407_109, 27_407_113
POT_NOW = 572_414_815
OLD_CLOSE1_AT = O1 + 816_480                  # the plan before this amendment


def accrued(e):
    return A1_PRINCIPAL * 11 * e // DEN


def stamp(unix):
    utc = datetime.fromtimestamp(unix, timezone.utc)
    return {"unix": unix, "utc": utc.strftime("%Y-%m-%dT%H:%M:%SZ"), "pacific": utc.astimezone(PT).strftime("%Y-%m-%d %H:%M:%S %Z")}


def half_to_only_row(toll):
    half = toll - toll // 2
    return sum(half * bps // 10_000 for bps in (3_500, 4_500, 2_000))


# Self-checks against the receipts.
f3_e = 358_784
assert accrued(f3_e) == F3_PAID and F3_PAID * 1_200 // 10_000 == F3_TOLL
assert half_to_only_row(F3_TOLL) == T1_CREDIT and F3_TOLL - T1_CREDIT == T1_SEED and POT0 - F3_PAID == POT_NOW

# The threshold: owed - pot > 0  <=>  accrued(e) > POT0 (F3 lowers both equally).
lo, hi = f3_e, 14 * DAY
while lo + 1 < hi:
    mid = (lo + hi) // 2
    lo, hi = (lo, mid) if accrued(mid) > POT0 else (mid, hi)
e_star = hi
delay = A1_OPEN - (GENESIS + O1)
new_close1_at = O1 + delay + e_star
assert (delay, e_star, new_close1_at, OLD_CLOSE1_AT - new_close1_at) == (132, 808_390, 981_562, 7_958)
assert GENESIS + new_close1_at == A1_OPEN + e_star == 1_793_923_857

# The close outcome: pot-capped, 7.5 %, identical at the new and the old instant.
def close(e):
    owed = accrued(e) - F3_PAID
    paid = min(owed, POT_NOW)
    toll = paid * 750 // 10_000
    return {"owed_minus_pot": owed - POT_NOW, "paid": paid, "toll": toll, "net": paid - toll, "insolvent": owed > POT_NOW}

at_new, before, at_old = close(e_star), close(e_star - 1), close(OLD_CLOSE1_AT + GENESIS - A1_OPEN)
assert not before["insolvent"] and at_new["insolvent"] and at_old["insolvent"]
assert {k: at_new[k] for k in ("paid", "toll", "net")} == {k: at_old[k] for k in ("paid", "toll", "net")}
t2_credit = half_to_only_row(at_new["toll"])


def schedule(c1, expire_after):
    r2o = c1 + 180
    p1 = r2o + 60
    c15, c30 = p1 + 1_296_060, r2o + 2_592_060
    return [
        ("a1_closing_withdraw", c1), ("advance_ticket_close1", c1 + 30), ("candidate_pages_1", c1 + 60),
        ("build_candidates_1", c1 + 90), ("request_draw_1", c1 + 120), ("fulfil_1", c1 + 135), ("capture_draw_1", c1 + 150),
        ("apply_promotion_1", c1 + 165), ("w_finalize_reopen", r2o), ("p1_open_compound15", p1), ("p2_open_simple", r2o + 120),
        ("p3_open_simple", r2o + 180), ("a2_fund_series_a", r2o + 240), ("winner_fund_series_a", r2o + 300),
        ("provision_close_record_2", r2o + 360), ("p1_settle_compound15", c15), ("advance_ticket_c15", c15 + 30),
        ("winner_claim_a_and_b", c15 + 90), ("a1_claim_after_cap", c15 + 150), ("w_settle_compound30", c30),
        ("advance_ticket_close2", c30 + 30), ("candidate_pages_2", c30 + 60), ("build_candidates_2", c30 + 90),
        ("request_draw_2", c30 + 120), ("expire_at_deadline", c30 + expire_after), ("expire_promotion_2", c30 + 601),
        ("finalize_2", c30 + 611),
    ]


old, new = schedule(OLD_CLOSE1_AT, 600), schedule(new_close1_at, 150)
rows = []
for index, ((name, before_at), (_, after_at)) in enumerate(zip(old, new), start=13):
    rows.append({"step": index, "id": name, "old_offset": before_at, "new_offset": after_at, "shift_s": after_at - before_at, **stamp(GENESIS + after_at)})
assert all(r["shift_s"] == -7_958 for r in rows if r["id"] != "expire_at_deadline")

print(json.dumps({
    "self_checks": "F3 paid/toll, ticket 1 credit/seed and the pot reproduce the receipts",
    "threshold": {"a1_open_delay_s": delay, "e_star_s": e_star, "owed_minus_pot_one_second_before": before["owed_minus_pot"],
                  "owed_minus_pot_at": at_new["owed_minus_pot"], "step13": stamp(GENESIS + new_close1_at),
                  "old_step13": stamp(GENESIS + OLD_CLOSE1_AT), "margin_removed_s": OLD_CLOSE1_AT - new_close1_at},
    "close1_unchanged": {"paid": at_new["paid"], "toll": at_new["toll"], "net": at_new["net"],
                         "ticket2_a1_credit": t2_credit, "ticket2_seed": at_new["toll"] - t2_credit,
                         "seed_carried_round2": T1_SEED + at_new["toll"] - t2_credit,
                         "a1_left_after_round1": 62_000_000 - T1_CREDIT - t2_credit},
    "schedule": rows,
    "duration_days": round(new[-1][1] / DAY, 3),
}, indent=1))
