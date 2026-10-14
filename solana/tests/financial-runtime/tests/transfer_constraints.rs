//! IMPL-43 step 1: what the local runtime can and cannot prove about the rule
//! that decides whether an owner refund is deliverable.
//!
//! The cluster rule (Agave `svm/src/rent_calculator.rs`, copied verbatim into
//! LiteSVM 0.7.1 `src/utils/rent.rs::transition_allowed`, commit 6fbbaf67):
//! a writable account may END a transaction rent-paying (0 < lamports <
//! rent-exempt minimum) only if it was rent-paying before, kept its data size
//! and was NOT credited.  For a data-less destination holding `c` lamports a
//! credit of `a > 0` therefore succeeds iff `c + a >= minimum(0)`.
//!
//! LiteSVM 0.7.1 applies that check only to accounts WITH data
//! (`check_accounts_rent` skips `account.data().is_empty()`), so system
//! wallets are never checked locally: every probe below SUCCEEDS in LiteSVM
//! although the cluster would reject three of them.  The program's
//! `credit_is_deliverable` is therefore validated against the rule text, not
//! against local execution; `local_runtime_does_not_check_rent_state_for_data_less_accounts`
//! pins the gap so a LiteSVM upgrade that closes it shows up here.
//!
//! RED before IMPL-43: the old predicate (`c > 0 || a >= minimum`) disagreed
//! with the rule for the legacy rent-paying case (c = 1, a = 500,000).

use musical_chairs_financial_runtime::{FinancialRuntime, Observation};
use musical_chairs_program::close::credit_is_deliverable;
use solana_sdk::{pubkey::Pubkey, signature::Signer};
use solana_system_interface::instruction as system_instruction;

const CU: u32 = 200_000;

struct Probe {
    name: &'static str,
    destination_before: u64,
    amount: u64,
    /// What the Agave rule says for a data-less destination.
    cluster_allows: bool,
}

fn probes(minimum: u64) -> Vec<Probe> {
    vec![
        Probe { name: "fresh_sub_minimum", destination_before: 0, amount: 500_000, cluster_allows: false },
        Probe { name: "legacy_rent_paying_stays_below", destination_before: 1, amount: 500_000, cluster_allows: false },
        Probe { name: "legacy_rent_paying_one_short", destination_before: 1, amount: minimum - 2, cluster_allows: false },
        Probe { name: "fresh_exact_minimum", destination_before: 0, amount: minimum, cluster_allows: true },
        Probe { name: "legacy_rent_paying_reaches_minimum", destination_before: 1, amount: minimum - 1, cluster_allows: true },
        Probe { name: "rent_exempt_any_credit", destination_before: minimum, amount: 1, cluster_allows: true },
    ]
}

/// The Agave rule restated for a data-less account credited by `amount > 0`:
/// the post state must be RentExempt (RentPaying is allowed only without a credit).
fn agave_rule_allows(current: u64, amount: u64, minimum: u64) -> bool {
    current.saturating_add(amount) >= minimum
}

fn run(rt: &mut FinancialRuntime, probe: &Probe) -> (Observation, u64) {
    let destination = Pubkey::new_unique();
    if probe.destination_before > 0 {
        rt.credit_lamports(destination, probe.destination_before);
    }
    let payer = rt.payer.pubkey();
    let ix = system_instruction::transfer(&payer, &destination, probe.amount);
    let observation = rt.execute(format!("transfer_constraints.{}", probe.name), vec![ix], &[], CU);
    let after = rt.svm.get_account(&destination).map_or(0, |a| a.lamports);
    (observation, after)
}

#[test]
fn local_runtime_does_not_check_rent_state_for_data_less_accounts() {
    let mut rt = FinancialRuntime::new();
    let minimum = rt.rent(0);
    assert!(minimum > 500_000, "the probes assume a rent-exempt minimum above 500,000 lamports (local: {minimum})");
    let mut cluster_rejects_but_local_accepts = 0;
    for probe in probes(minimum) {
        let (observation, after) = run(&mut rt, &probe);
        println!(
            "PROBE {} before={} amount={} cluster_allows={} litesvm_success={} error={:?} after={}",
            probe.name, probe.destination_before, probe.amount, probe.cluster_allows, observation.success, observation.error, after
        );
        assert!(observation.success, "{}: LiteSVM 0.7.1 accepted every data-less credit when this was written; if this fails the local runtime now enforces the rule and the module docs must change", probe.name);
        assert_eq!(after, probe.destination_before + probe.amount, "{}", probe.name);
        if !probe.cluster_allows {
            cluster_rejects_but_local_accepts += 1;
        }
    }
    assert_eq!(cluster_rejects_but_local_accepts, 3, "three probes are cluster-rejected yet locally accepted");
}

/// GREEN only with the IMPL-43 predicate: for every probe and a dense grid
/// around the minimum, the program's predicate equals the Agave rule.
#[test]
fn predicate_matches_the_agave_rent_state_rule() {
    let minimum = FinancialRuntime::new().rent(0);
    let mut disagreements = Vec::new();
    for probe in probes(minimum) {
        let predicted = credit_is_deliverable(probe.destination_before, probe.amount, minimum);
        if predicted != probe.cluster_allows {
            disagreements.push(format!("{}: predicate {predicted}, rule {}", probe.name, probe.cluster_allows));
        }
    }
    for current in [0u64, 1, 2, minimum / 2, minimum - 1, minimum, minimum + 1, 10 * minimum] {
        for amount in [1u64, 2, minimum / 2, minimum - 2, minimum - 1, minimum, minimum + 1] {
            let (predicted, rule) = (credit_is_deliverable(current, amount, minimum), agave_rule_allows(current, amount, minimum));
            if predicted != rule {
                disagreements.push(format!("current={current} amount={amount}: predicate {predicted}, rule {rule}"));
            }
        }
    }
    assert!(disagreements.is_empty(), "credit_is_deliverable disagrees with the Agave rent-state rule:\n{}", disagreements.join("\n"));
}
