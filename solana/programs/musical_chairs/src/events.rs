//! Program events (spec section 7.5), emitted with `emit_cpi!`.
//!
//! Wire format of one emitted event: the self-CPI instruction data is
//! `EVENT_IX_TAG_LE || discriminator || borsh(body)`, where the discriminator
//! is `sha256("event:<StructName>")[..8]`.  Indexers read it from the inner
//! instruction of the emitting transaction, which (unlike a log line) cannot
//! be truncated by the runtime log limit.
//!
//! Field order and widths are an off-chain ABI.  Never reorder or retype a
//! field in place: add a new event, or bump `SCHEMA_VERSION` and publish the
//! new layout.  Every event carries `schema_version` as its first field.
use anchor_lang::prelude::*;

/// Current layout version, written into the first field of every event.
pub const SCHEMA_VERSION: u8 = 1;

/// Ingested by the ICP bridge, which compares descriptor fields against
/// `round_id` (and `close_id`) only; `close_id` is always `None` here.
#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionOpened {
    pub schema_version: u8,
    pub owner: Pubkey,
    pub nonce: u64,
    pub round_id: u64,
    /// Always `None`: an opening is never attributed to a close.
    pub close_id: Option<u64>,
    pub plan: u8,
    pub gross: u64,
    pub load: u64,
    pub config_version: u32,
}

/// Ingested by the ICP bridge, which compares descriptor fields against
/// `round_id` (and `close_id`) only; `close_id` is always `None` here.
#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesAFunded {
    pub schema_version: u8,
    pub owner: Pubkey,
    pub round_id: u64,
    /// Always `None`: funding is never attributed to a close.
    pub close_id: Option<u64>,
    pub gross: u64,
    pub entitlement_after: u128,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutPaid {
    pub schema_version: u8,
    pub owner: Pubkey,
    pub nonce: u64,
    pub round_id: u64,
    pub gross_paid: u64,
    pub toll: u64,
    pub net: u64,
    pub destination: Pubkey,
    pub closing: bool,
    pub close_id: Option<u64>,
    pub ticket_id: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionClosed {
    pub schema_version: u8,
    pub owner: Pubkey,
    pub nonce: u64,
    pub round_id: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketCommitted {
    pub schema_version: u8,
    pub ticket_id: u64,
    pub round_id: u64,
    pub toll: u64,
    pub credited: u64,
    pub seed: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackerClaimed {
    pub schema_version: u8,
    pub owner: Pubkey,
    pub round_id: u64,
    pub amount: u64,
    pub a_part: u64,
    pub b_part: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundClosed {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub deadline: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidatesSealed {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub tag: u8,
    pub count: u64,
    pub digest: [u8; 32],
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawRequested {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub request_account: Pubkey,
    pub fee_paid: u64,
    pub request_rent: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawCaptured {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub captured_at: u64,
}

/// Ingested by the ICP bridge, which compares descriptor fields against
/// `round_id` and `close_id` only; `close_id` is always `Some` here.
#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionApplied {
    pub schema_version: u8,
    pub round_id: u64,
    /// Always `Some`: a promotion belongs to exactly one close.
    pub close_id: Option<u64>,
    pub winner: Pubkey,
    pub loss: u64,
    pub entitlement: u128,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionSkipped {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub reason: u8,
    pub stage: u8,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundFinalized {
    pub schema_version: u8,
    pub close_id: u64,
    pub new_round: u64,
    pub seed_carried: u64,
}

/// A deferred owner refund (unused close reserve, or returned owner-funded
/// request rent, that the runtime could not deliver at settlement) reached the
/// immutable owner destination through `deliver_owner_refund`.
#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerRefundDelivered {
    pub schema_version: u8,
    pub round_id: u64,
    pub close_id: u64,
    pub amount: u64,
    pub destination: Pubkey,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationsFunded {
    pub schema_version: u8,
    pub funder: Pubkey,
    pub amount: u64,
    pub free_balance_after: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationsWithdrawn {
    pub schema_version: u8,
    pub recipient: Pubkey,
    pub amount: u64,
    pub free_balance_after: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagementWithdrawn {
    pub schema_version: u8,
    pub recipient: Pubkey,
    pub amount: u64,
    pub management_after: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigStaged {
    pub schema_version: u8,
    pub version: u32,
    pub effective_round_id: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigActivated {
    pub schema_version: u8,
    pub version: u32,
    pub round_id: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionPauseSet {
    pub schema_version: u8,
    pub paused: bool,
    pub guardian_vault: Pubkey,
    pub changed_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::{AnchorDeserialize, AnchorSerialize, Discriminator, Event};
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/event-encodings.json");

    fn key(n: u8) -> Pubkey {
        Pubkey::new_from_array([n; 32])
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            write!(out, "{b:02x}").expect("writing to a String cannot fail");
            out
        })
    }

    /// Renders a sample field value as a stable string for the fixture.
    trait Render {
        fn render(&self) -> String;
    }
    macro_rules! render_display {
        ($($t:ty),*) => {$(impl Render for $t {
            fn render(&self) -> String { self.to_string() }
        })*};
    }
    render_display!(u8, u32, u64, u128, bool, Pubkey);
    impl Render for Option<u64> {
        fn render(&self) -> String {
            match self {
                None => "None".to_string(),
                Some(v) => format!("Some({v})"),
            }
        }
    }
    impl Render for [u8; 32] {
        fn render(&self) -> String {
            hex(self)
        }
    }

    struct Case {
        name: &'static str,
        discriminator: Vec<u8>,
        sample: Vec<(&'static str, String)>,
        body: Vec<u8>,
        data: Vec<u8>,
        cpi_ix_data: Option<Vec<u8>>,
    }

    fn record<E>(name: &'static str, event: &E, sample: Vec<(&'static str, String)>, ingested_by_bridge: bool) -> Case
    where
        E: Event + Discriminator + AnchorSerialize + AnchorDeserialize + PartialEq + std::fmt::Debug,
    {
        let discriminator = E::DISCRIMINATOR.to_vec();
        let data = event.data();

        let mut body = Vec::new();
        event.serialize(&mut body).expect("borsh serialization of a fixed-size event");
        assert_eq!(&data[..8], discriminator.as_slice(), "{name}: data must start with the discriminator");
        assert_eq!(&data[8..], body.as_slice(), "{name}: data must be discriminator || borsh(body)");
        assert_eq!(&E::deserialize(&mut body.as_slice()).expect("round-trip"), event, "{name}: borsh round-trip");

        let expected = Sha256::digest(format!("event:{name}"));
        assert_eq!(discriminator.as_slice(), &expected[..8], "{name}: discriminator is sha256(\"event:{name}\")[..8]");

        let cpi_ix_data = ingested_by_bridge.then(|| {
            let mut ix = anchor_lang::event::EVENT_IX_TAG_LE.to_vec();
            ix.extend_from_slice(&data);
            ix
        });
        Case { name, discriminator, sample, body, data, cpi_ix_data }
    }

    macro_rules! case {
        ($cases:ident, $bridge:expr, $ev:ident { $($f:ident : $v:expr),* $(,)? }) => {{
            let event = $ev { $($f: $v),* };
            let sample = vec![$((stringify!($f), Render::render(&event.$f))),*];
            $cases.push(record(stringify!($ev), &event, sample, $bridge));
        }};
    }

    fn all_cases() -> Vec<Case> {
        let v = SCHEMA_VERSION;
        let mut cases = Vec::new();
        case!(cases, true, PositionOpened {
            schema_version: v, owner: key(1), nonce: 11, round_id: 12, close_id: None,
            plan: 2, gross: 13, load: 14, config_version: 15,
        });
        case!(cases, true, SeriesAFunded {
            schema_version: v, owner: key(2), round_id: 21, close_id: None, gross: 22,
            entitlement_after: 23,
        });
        case!(cases, false, PayoutPaid {
            schema_version: v, owner: key(3), nonce: 31, round_id: 32, gross_paid: 33, toll: 34,
            net: 35, destination: key(4), closing: true, close_id: Some(36), ticket_id: 37,
        });
        case!(cases, false, PositionClosed {
            schema_version: v, owner: key(5), nonce: 41, round_id: 42,
        });
        case!(cases, false, TicketCommitted {
            schema_version: v, ticket_id: 51, round_id: 52, toll: 53, credited: 54, seed: 55,
        });
        case!(cases, false, BackerClaimed {
            schema_version: v, owner: key(6), round_id: 61, amount: 62, a_part: 63, b_part: 64,
        });
        case!(cases, false, RoundClosed {
            schema_version: v, round_id: 71, close_id: 72, deadline: 73,
        });
        case!(cases, false, CandidatesSealed {
            schema_version: v, round_id: 81, close_id: 82, tag: 3, count: 83, digest: [0xab; 32],
        });
        case!(cases, false, DrawRequested {
            schema_version: v, round_id: 91, close_id: 92, request_account: key(7), fee_paid: 93,
            request_rent: 94,
        });
        case!(cases, false, DrawCaptured {
            schema_version: v, round_id: 101, close_id: 102, captured_at: 103,
        });
        case!(cases, true, PromotionApplied {
            schema_version: v, round_id: 111, close_id: Some(112), winner: key(8), loss: 113,
            entitlement: 114,
        });
        case!(cases, false, PromotionSkipped {
            schema_version: v, round_id: 121, close_id: 122, reason: 4, stage: 5,
        });
        case!(cases, false, RoundFinalized {
            schema_version: v, close_id: 131, new_round: 132, seed_carried: 133,
        });
        case!(cases, false, OwnerRefundDelivered {
            schema_version: v, round_id: 201, close_id: 202, amount: 203, destination: key(13),
        });
        case!(cases, false, OperationsFunded {
            schema_version: v, funder: key(9), amount: 141, free_balance_after: 142,
        });
        case!(cases, false, OperationsWithdrawn {
            schema_version: v, recipient: key(10), amount: 151, free_balance_after: 152,
        });
        case!(cases, false, ManagementWithdrawn {
            schema_version: v, recipient: key(11), amount: 161, management_after: 162,
        });
        case!(cases, false, ConfigStaged {
            schema_version: v, version: 171, effective_round_id: 172,
        });
        case!(cases, false, ConfigActivated {
            schema_version: v, version: 181, round_id: 182,
        });
        case!(cases, false, AdmissionPauseSet {
            schema_version: v, paused: true, guardian_vault: key(12), changed_at: 191,
        });
        cases.sort_by_key(|c| c.name);
        cases
    }

    /// Strings in the fixture are plain ASCII (digits, hex, base58, `Some(n)`);
    /// reject anything that would need JSON escaping instead of escaping it.
    fn json_str(s: &str) -> String {
        assert!(s.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\'), "unescapable fixture string {s:?}");
        format!("\"{s}\"")
    }

    fn render_json(cases: &[Case]) -> String {
        let mut out = String::from("[\n");
        for (i, c) in cases.iter().enumerate() {
            out.push_str("  {\n");
            writeln!(out, "    \"name\": {},", json_str(c.name)).unwrap();
            writeln!(out, "    \"discriminator_hex\": {},", json_str(&hex(&c.discriminator))).unwrap();
            out.push_str("    \"sample\": {\n");
            for (j, (field, value)) in c.sample.iter().enumerate() {
                let comma = if j + 1 < c.sample.len() { "," } else { "" };
                writeln!(out, "      {}: {}{comma}", json_str(field), json_str(value)).unwrap();
            }
            out.push_str("    },\n");
            writeln!(out, "    \"body_hex\": {},", json_str(&hex(&c.body))).unwrap();
            match &c.cpi_ix_data {
                Some(ix) => {
                    writeln!(out, "    \"data_hex\": {},", json_str(&hex(&c.data))).unwrap();
                    writeln!(out, "    \"event_cpi_ix_data_hex\": {}", json_str(&hex(ix))).unwrap();
                }
                None => writeln!(out, "    \"data_hex\": {}", json_str(&hex(&c.data))).unwrap(),
            }
            out.push_str(if i + 1 < cases.len() { "  },\n" } else { "  }\n" });
        }
        out.push_str("]\n");
        out
    }

    #[test]
    fn every_event_leads_with_schema_version_and_all_nineteen_exist() {
        let cases = all_cases();
        assert_eq!(cases.len(), 20);
        for c in &cases {
            assert_eq!(c.sample[0], ("schema_version", SCHEMA_VERSION.to_string()), "{}", c.name);
            assert_eq!(c.body[0], SCHEMA_VERSION, "{}: schema_version must be the first body byte", c.name);
        }
        let mut names: Vec<_> = cases.iter().map(|c| c.name).collect();
        names.dedup();
        assert_eq!(names.len(), 20, "event names must be unique");
    }

    #[test]
    fn event_cpi_tag_is_the_anchor_constant_in_little_endian() {
        assert_eq!(anchor_lang::event::EVENT_IX_TAG, 0x1d9a_cb51_2ea5_45e4);
        assert_eq!(anchor_lang::event::EVENT_IX_TAG_LE, &[0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d]);
    }

    #[test]
    fn pp_ingested_events_pin_their_close_id_shape() {
        let cases = all_cases();
        let find = |name: &str| cases.iter().find(|c| c.name == name).unwrap();
        let sample = |name: &str, field: &str| {
            find(name).sample.iter().find(|(f, _)| *f == field).unwrap().1.clone()
        };
        assert_eq!(sample("PositionOpened", "close_id"), "None");
        assert_eq!(sample("SeriesAFunded", "close_id"), "None");
        assert!(sample("PromotionApplied", "close_id").starts_with("Some("));
        let bridged: Vec<_> = cases.iter().filter(|c| c.cpi_ix_data.is_some()).map(|c| c.name).collect();
        assert_eq!(bridged, ["PositionOpened", "PromotionApplied", "SeriesAFunded"]);
    }

    /// With `MC_WRITE_EVENT_FIXTURE=1` this (re)writes the checked-in fixture;
    /// otherwise it asserts the program still produces exactly those bytes.
    #[test]
    fn event_encodings_match_the_checked_in_fixture() {
        let rendered = render_json(&all_cases());
        if std::env::var("MC_WRITE_EVENT_FIXTURE").as_deref() == Ok("1") {
            std::fs::write(FIXTURE_PATH, &rendered).expect("write event-encodings.json");
            return;
        }
        let on_disk = std::fs::read_to_string(FIXTURE_PATH)
            .expect("solana/fixtures/event-encodings.json is missing; regenerate with MC_WRITE_EVENT_FIXTURE=1");
        assert_eq!(on_disk, rendered, "event encodings drifted from solana/fixtures/event-encodings.json");
    }
}
