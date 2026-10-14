//! Pure sampling and hashing rules for the promotion draw (spec section 4).
//!
//! Three byte-exact rules live here, shared by the program and by every
//! off-chain verifier:
//!
//! * the ordinal mapping of a 64-byte provider output to one candidate
//!   (rejection sampling over counters `0..64`, no modulo bias);
//! * the protocol seed bound into the provider request, which pins cluster,
//!   program, round, close, candidate set and sealed digest;
//! * the chained candidate digest, which fits the 32 bytes `CloseRecord`
//!   stores and commits to the ordered candidate list.
//!
//! Hashing is SHA-256: the `sol_sha256` syscall on SBF and the `sha2` crate
//! on the host (the 2.x `hashv` panics off-chain without its `sha2` feature).
//! `solana/fixtures/sampling.json` pins exact vectors for all three rules.

use anchor_lang::prelude::Pubkey;

pub const INDEX_DOMAIN: &[u8] = b"MC/EEC/INDEX/v1";
pub const SEED_DOMAIN: &[u8] = b"SchemeWorks/MC/EEC/v1";
pub const CANDIDATE_DOMAIN: &[u8] = b"MC/EEC/CANDIDATES/v1";
/// Counters tried before a draw is declared entropy-exhausted.
pub const MAX_COUNTERS: u64 = 64;

/// SHA-256 over the concatenation of `parts`.  One code path for both targets:
/// `solana_sha256_hasher::hashv` is the `sol_sha256` syscall on SBF and `sha2`
/// on the host, so the unit tests exercise the exact function the program runs
/// (anchor-lang 0.32 exposes no `solana_program::hash` facade; the earlier
/// SBF-only branch did not compile under cargo-build-sbf).
pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    solana_sha256_hasher::hashv(parts).to_bytes()
}

/// Rejection sampler over `candidate_count` ordinals. `sample(counter)` yields
/// the 64-bit word for that counter; the first word below
/// `limit = floor(2^64 / N) * N` is accepted and reduced mod `N`. `None` when
/// `N == 0` or all 64 counters are rejected (entropy exhausted).
pub fn map_samples(candidate_count: u64, mut sample: impl FnMut(u64) -> u64) -> Option<(u64, u64)> {
    if candidate_count == 0 {
        return None;
    }
    let n = u128::from(candidate_count);
    let limit = (1u128 << 64) / n * n;
    for counter in 0..MAX_COUNTERS {
        let word = u128::from(sample(counter));
        if word < limit {
            return Some((u64::try_from(word % n).ok()?, counter));
        }
    }
    None
}

/// Map a provider output to `(ordinal, counter)`:
/// `word_c = u64_le(SHA256("MC/EEC/INDEX/v1" || output || round_id_le || c_le)[0..8])`.
pub fn map_output(output: [u8; 64], round_id: u64, candidate_count: u64) -> Option<(u64, u64)> {
    let round = round_id.to_le_bytes();
    map_samples(candidate_count, |counter| {
        let digest = sha256(&[INDEX_DOMAIN, &output, &round, &counter.to_le_bytes()]);
        let mut word = [0u8; 8];
        word.copy_from_slice(&digest[..8]);
        u64::from_le_bytes(word)
    })
}

/// `SHA256("SchemeWorks/MC/EEC/v1" || cluster_domain || program_id || round_id_le
/// || close_id_le || set_tag || selected_count_le || sealed_digest)`.
pub fn protocol_seed(
    cluster_domain: &[u8; 32],
    program_id: &Pubkey,
    round_id: u64,
    close_id: u64,
    set_tag: u8,
    selected_count: u64,
    sealed_digest: &[u8; 32],
) -> [u8; 32] {
    sha256(&[
        SEED_DOMAIN,
        cluster_domain,
        program_id.as_ref(),
        &round_id.to_le_bytes(),
        &close_id.to_le_bytes(),
        &[set_tag],
        &selected_count.to_le_bytes(),
        sealed_digest,
    ])
}

/// Digest of an empty candidate set: `SHA256("MC/EEC/CANDIDATES/v1" || set_tag)`.
pub fn candidate_digest_start(set_tag: u8) -> [u8; 32] {
    sha256(&[CANDIDATE_DOMAIN, &[set_tag]])
}

/// Append one candidate: `SHA256(previous || owner || loss_le(16))`.
pub fn candidate_digest_next(previous: &[u8; 32], owner: &Pubkey, loss: u128) -> [u8; 32] {
    sha256(&[previous, owner.as_ref(), &loss.to_le_bytes()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/sampling.json");
    const PROGRAM_ID: &str = "7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7";
    const LITESVM_DOMAIN: [u8; 32] = *b"musical-chairs-litesvm-fixture!!";
    const DEVNET_DOMAIN: [u8; 32] = *b"musical-chairs-devnet-domain-v1!";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            write!(out, "{b:02x}").expect("writing to a String cannot fail");
            out
        })
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// Independent reference for the ordinal mapping, written against the
    /// `sha2` crate directly so it shares nothing with the module under test.
    /// `economics::rounds::map_output_for_test` is `#[cfg(test)]` inside the
    /// economics crate and therefore unreachable from here; the fixture was
    /// cross-checked against it with a one-off test in that crate.
    fn reference_map_output(output: [u8; 64], round_id: u64, n: u64) -> Option<(u64, u64)> {
        use sha2::{Digest, Sha256};
        if n == 0 {
            return None;
        }
        let limit = (1u128 << 64) / u128::from(n) * u128::from(n);
        for counter in 0..64u64 {
            let mut hasher = Sha256::new();
            hasher.update(b"MC/EEC/INDEX/v1");
            hasher.update(output);
            hasher.update(round_id.to_le_bytes());
            hasher.update(counter.to_le_bytes());
            let digest = hasher.finalize();
            let r = u64::from_le_bytes(digest[..8].try_into().unwrap());
            if u128::from(r) < limit {
                return Some(((u128::from(r) % u128::from(n)) as u64, counter));
            }
        }
        None
    }

    fn output_of(seed: u8) -> [u8; 64] {
        let mut out = [0u8; 64];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = seed.wrapping_mul(31).wrapping_add(i as u8);
        }
        out
    }

    /// N just above 2^63: `limit == N`, so each word is rejected with
    /// probability about one half and a counter above zero is common.
    const LARGE_N: u64 = (1u64 << 63) + 1;

    /// First output (by scan) whose accepted counter for `n` is `target`.
    fn find_output(n: u64, round_id: u64, target: u64) -> [u8; 64] {
        (0u8..=255)
            .map(output_of)
            .find(|out| matches!(map_output(*out, round_id, n), Some((_, c)) if c == target))
            .expect("a small scan finds a vector with the wanted counter")
    }

    struct OutputVector {
        label: &'static str,
        output: [u8; 64],
        round_id: u64,
        n: u64,
    }

    fn output_vectors() -> Vec<OutputVector> {
        vec![
            OutputVector { label: "zero output, one candidate", output: [0; 64], round_id: 1, n: 1 },
            OutputVector { label: "zero output, two candidates", output: [0; 64], round_id: 1, n: 2 },
            OutputVector { label: "ramp output, three candidates", output: output_of(1), round_id: 7, n: 3 },
            OutputVector { label: "ramp output, twenty candidates", output: output_of(2), round_id: 1, n: 20 },
            OutputVector { label: "all-ones output, 1000 candidates", output: [0xFF; 64], round_id: u64::MAX, n: 1_000 },
            OutputVector { label: "round id matters", output: output_of(3), round_id: 2, n: 20 },
            OutputVector { label: "large N, counter zero accepted", output: find_output(LARGE_N, 1, 0), round_id: 1, n: LARGE_N },
            OutputVector { label: "large N, counter one accepted", output: find_output(LARGE_N, 1, 1), round_id: 1, n: LARGE_N },
            OutputVector { label: "large N, counter three accepted", output: find_output(LARGE_N, 1, 3), round_id: 1, n: LARGE_N },
            OutputVector { label: "N = u64::MAX", output: output_of(9), round_id: 1, n: u64::MAX },
        ]
    }

    /// Four fixed candidates for the digest and seed vectors.
    fn fixed_candidates() -> [(Pubkey, u128); 4] {
        [(key(1), 100), (key(2), 200), (key(3), 300_000_000), (key(4), u128::from(u64::MAX) + 7)]
    }

    fn fixed_digest(set_tag: u8) -> [u8; 32] {
        fixed_candidates()
            .iter()
            .fold(candidate_digest_start(set_tag), |digest, (owner, loss)| {
                candidate_digest_next(&digest, owner, *loss)
            })
    }

    fn seed_for(domain: &[u8; 32]) -> [u8; 32] {
        protocol_seed(domain, &crate::ID, 1, 1, 0, 4, &fixed_digest(0))
    }

    fn render_output(label: &str, v: &OutputVector) -> String {
        let (ordinal, counter) = match map_output(v.output, v.round_id, v.n) {
            Some((ordinal, counter)) => (ordinal.to_string(), counter.to_string()),
            None => ("null".to_string(), "null".to_string()),
        };
        format!(
            "    {{\n      \"label\": \"{label}\",\n      \"output_hex\": \"{}\",\n      \"round_id\": {},\n      \"candidate_count\": {},\n      \"ordinal\": {ordinal},\n      \"counter\": {counter}\n    }}",
            hex(&v.output),
            v.round_id,
            v.n,
        )
    }

    fn render_json() -> String {
        let outputs: Vec<String> =
            output_vectors().iter().map(|v| render_output(v.label, v)).collect();
        let candidates: Vec<String> = fixed_candidates()
            .iter()
            .map(|(owner, loss)| {
                format!("    {{ \"owner\": \"{owner}\", \"owner_hex\": \"{}\", \"loss\": \"{loss}\" }}", hex(owner.as_ref()))
            })
            .collect();
        format!(
            concat!(
                "{{\n",
                "  \"description\": \"Byte-exact vectors for the promotion draw: ordinal mapping, candidate digest chain and protocol seed. Regenerate with MC_WRITE_SAMPLING_FIXTURE=1 cargo test sampling.\",\n",
                "  \"domains\": {{\n",
                "    \"index\": \"MC/EEC/INDEX/v1\",\n",
                "    \"seed\": \"SchemeWorks/MC/EEC/v1\",\n",
                "    \"candidates\": \"MC/EEC/CANDIDATES/v1\"\n",
                "  }},\n",
                "  \"rule\": {{\n",
                "    \"word\": \"u64_le(SHA256(index_domain || output_64 || round_id_le || counter_le)[0..8])\",\n",
                "    \"limit\": \"floor(2^64 / N) * N\",\n",
                "    \"accept\": \"first counter in 0..64 with word < limit; ordinal = word mod N\",\n",
                "    \"exhausted\": \"all 64 counters rejected (or N == 0)\"\n",
                "  }},\n",
                "  \"output_vectors\": [\n{outputs}\n  ],\n",
                "  \"constructed_samples\": [\n",
                "    {{ \"label\": \"all 64 words rejected, N = 3 (limit = 2^64 - 1)\", \"candidate_count\": 3, \"word_for_every_counter\": \"18446744073709551615\", \"result\": \"exhausted\" }},\n",
                "    {{ \"label\": \"counter 0 rejected, counter 1 accepted, N = 2^63 + 1 (limit = N)\", \"candidate_count\": {large}, \"words\": [\"{large}\", \"5\"], \"counter\": 1, \"ordinal\": 5 }}\n",
                "  ],\n",
                "  \"candidates\": {{\n",
                "    \"set_tag\": 0,\n",
                "    \"rows\": [\n{candidates}\n    ],\n",
                "    \"empty_digest_hex\": \"{empty_digest}\",\n",
                "    \"chain_rule\": \"d0 = SHA256(candidates_domain || tag); d(i+1) = SHA256(d(i) || owner_32 || loss_le_16)\",\n",
                "    \"digest_hex\": \"{digest}\"\n",
                "  }},\n",
                "  \"protocol_seed\": {{\n",
                "    \"rule\": \"SHA256(seed_domain || cluster_domain_32 || program_id_32 || round_le || close_le || set_tag || selected_count_le || sealed_digest_32)\",\n",
                "    \"program_id\": \"{program}\",\n",
                "    \"round_id\": 1,\n",
                "    \"close_id\": 1,\n",
                "    \"set_tag\": 0,\n",
                "    \"selected_count\": 4,\n",
                "    \"sealed_digest_hex\": \"{digest}\",\n",
                "    \"vectors\": [\n",
                "      {{ \"cluster\": \"litesvm-fixture\", \"cluster_domain\": \"{litesvm}\", \"cluster_domain_hex\": \"{litesvm_hex}\", \"seed_hex\": \"{litesvm_seed}\" }},\n",
                "      {{ \"cluster\": \"devnet\", \"cluster_domain\": \"{devnet}\", \"cluster_domain_hex\": \"{devnet_hex}\", \"seed_hex\": \"{devnet_seed}\" }}\n",
                "    ]\n",
                "  }}\n",
                "}}\n",
            ),
            outputs = outputs.join(",\n"),
            large = LARGE_N,
            candidates = candidates.join(",\n"),
            empty_digest = hex(&candidate_digest_start(0)),
            digest = hex(&fixed_digest(0)),
            program = crate::ID,
            litesvm = String::from_utf8_lossy(&LITESVM_DOMAIN),
            litesvm_hex = hex(&LITESVM_DOMAIN),
            litesvm_seed = hex(&seed_for(&LITESVM_DOMAIN)),
            devnet = String::from_utf8_lossy(&DEVNET_DOMAIN),
            devnet_hex = hex(&DEVNET_DOMAIN),
            devnet_seed = hex(&seed_for(&DEVNET_DOMAIN)),
        )
    }

    #[test]
    fn program_id_in_the_fixture_is_the_declared_id() {
        assert_eq!(crate::ID.to_string(), PROGRAM_ID);
    }

    #[test]
    fn mapping_is_byte_identical_to_the_independent_reference() {
        for v in output_vectors() {
            assert_eq!(
                map_output(v.output, v.round_id, v.n),
                reference_map_output(v.output, v.round_id, v.n),
                "{}",
                v.label
            );
        }
        // Dense sweep, including N values where the limit is not 2^64.
        for seed in 0u8..64 {
            for n in [1u64, 2, 3, 5, 7, 20, 255, 1_000, 1 << 32, LARGE_N, u64::MAX] {
                assert_eq!(
                    map_output(output_of(seed), u64::from(seed), n),
                    reference_map_output(output_of(seed), u64::from(seed), n),
                );
            }
        }
    }

    #[test]
    fn every_ordinal_is_below_the_candidate_count_and_counter_below_64() {
        for v in output_vectors() {
            let (ordinal, counter) = map_output(v.output, v.round_id, v.n).expect("accepted");
            assert!(ordinal < v.n && counter < MAX_COUNTERS, "{}", v.label);
        }
    }

    #[test]
    fn zero_candidates_never_map() {
        assert_eq!(map_output([7; 64], 1, 0), None);
        assert_eq!(map_samples(0, |_| 0), None);
    }

    #[test]
    fn one_candidate_always_maps_to_ordinal_zero_at_counter_zero() {
        for seed in 0u8..16 {
            assert_eq!(map_output(output_of(seed), 3, 1), Some((0, 0)));
        }
    }

    #[test]
    fn large_n_vectors_exercise_nonzero_counters() {
        for target in [0u64, 1, 3] {
            let out = find_output(LARGE_N, 1, target);
            assert_eq!(map_output(out, 1, LARGE_N).map(|(_, c)| c), Some(target));
        }
    }

    #[test]
    fn constructed_words_reject_all_64_counters() {
        // N = 3: limit = 2^64 - 1, so the all-ones word is rejected every time.
        let mut seen = Vec::new();
        assert_eq!(
            map_samples(3, |counter| {
                seen.push(counter);
                u64::MAX
            }),
            None
        );
        assert_eq!(seen, (0..64).collect::<Vec<_>>(), "all 64 counters are tried, in order");
        // N = 2^63 + 1: limit = N, so every word at or above N is rejected.
        assert_eq!(map_samples(LARGE_N, |_| u64::MAX), None);
        assert_eq!(map_samples(LARGE_N, |_| LARGE_N), None);
        // The word just below the limit is accepted at counter zero.
        assert_eq!(map_samples(LARGE_N, |_| LARGE_N - 1), Some((LARGE_N - 1, 0)));
    }

    #[test]
    fn first_accepted_counter_wins_and_later_counters_are_not_sampled() {
        let mut calls = 0;
        let result = map_samples(LARGE_N, |counter| {
            calls += 1;
            if counter == 0 { u64::MAX } else { 5 }
        });
        assert_eq!(result, Some((5, 1)));
        assert_eq!(calls, 2);
    }

    #[test]
    fn candidate_digest_chain_is_deterministic_and_order_sensitive() {
        assert_eq!(fixed_digest(0), fixed_digest(0));
        assert_ne!(fixed_digest(0), fixed_digest(1), "the set tag is bound");
        let forward = fixed_digest(0);
        let mut reordered = candidate_digest_start(0);
        for (owner, loss) in fixed_candidates().iter().rev() {
            reordered = candidate_digest_next(&reordered, owner, *loss);
        }
        assert_ne!(forward, reordered, "order is bound");
        let altered = candidate_digest_next(&candidate_digest_start(0), &key(1), 101);
        assert_ne!(altered, candidate_digest_next(&candidate_digest_start(0), &key(1), 100), "loss is bound");
        assert_ne!(candidate_digest_start(0), [0u8; 32]);
    }

    #[test]
    fn protocol_seed_binds_every_field() {
        let digest = fixed_digest(0);
        let base = protocol_seed(&LITESVM_DOMAIN, &crate::ID, 1, 1, 0, 4, &digest);
        let variants = [
            protocol_seed(&DEVNET_DOMAIN, &crate::ID, 1, 1, 0, 4, &digest),
            protocol_seed(&LITESVM_DOMAIN, &key(9), 1, 1, 0, 4, &digest),
            protocol_seed(&LITESVM_DOMAIN, &crate::ID, 2, 1, 0, 4, &digest),
            protocol_seed(&LITESVM_DOMAIN, &crate::ID, 1, 2, 0, 4, &digest),
            protocol_seed(&LITESVM_DOMAIN, &crate::ID, 1, 1, 1, 4, &digest),
            protocol_seed(&LITESVM_DOMAIN, &crate::ID, 1, 1, 0, 5, &digest),
            protocol_seed(&LITESVM_DOMAIN, &crate::ID, 1, 1, 0, 4, &fixed_digest(1)),
        ];
        for (i, variant) in variants.iter().enumerate() {
            assert_ne!(*variant, base, "field {i} must change the seed");
        }
        assert_ne!(seed_for(&LITESVM_DOMAIN), seed_for(&DEVNET_DOMAIN), "devnet and fixture clusters differ");
    }

    /// With `MC_WRITE_SAMPLING_FIXTURE=1` this (re)writes the checked-in
    /// fixture; otherwise it asserts the program still produces those bytes.
    #[test]
    fn sampling_vectors_match_the_checked_in_fixture() {
        let rendered = render_json();
        if std::env::var("MC_WRITE_SAMPLING_FIXTURE").as_deref() == Ok("1") {
            std::fs::write(FIXTURE_PATH, &rendered).expect("write sampling.json");
            return;
        }
        let on_disk = std::fs::read_to_string(FIXTURE_PATH)
            .expect("solana/fixtures/sampling.json is missing; regenerate with MC_WRITE_SAMPLING_FIXTURE=1");
        assert_eq!(on_disk, rendered, "sampling vectors drifted from solana/fixtures/sampling.json");
    }
}
