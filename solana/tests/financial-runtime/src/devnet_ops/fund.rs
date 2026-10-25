//! `devnet_ops fund-wallets` (M14-FUND-TOOL): top up the plan wallets (voters, sponsor, management
//! recipient, keeper, the eight test wallets) from the deployer's remainder, each ONCE, under the packet's
//! envelope (4b bucket caps, 4c: one transfer between devnet keys is at most 2 SOL).
//!
//! ```text
//! devnet_ops fund-wallets --payer <keypair file> --plan <plan.json> [--plan-commit <40-hex commit>] [--execute ...]
//! ```
//!
//! # What binds the plan
//!
//! On devnet `--execute` the plan file AND the inventory file it names (the `inventory` path inside the plan)
//! must each equal their blob at `--plan-commit` (`git cat-file blob <commit>:<path>`, the working file
//! byte-equal): the same artifact-identity idea as the packet pin, and equally NOT an approval (Rob's explicit
//! approval in chat is the authority).  Path, commit and SHA-256 of both go into the receipt and into every
//! write-ahead record.  A loopback run may omit `--plan-commit` (a given one is still verified).
//!
//! # What is refused before the first send (zero sends)
//!
//! Anything wrong anywhere in the WHOLE plan: schema, network, genesis, payer, an unknown or duplicated role or
//! pubkey, a target that is the payer, a role whose pubkey is not the inventory's (no aliases), a bucket that
//! is unknown or not the role's, a target of zero / above the per-transfer cap / below the rent floor, a cap the
//! plan claims above the packet's, a bucket or the command total above its cap, any arithmetic overflow, a
//! recipient that is not a plain system wallet, an unreadable or unresolved write-ahead record for a target, or a
//! payer that cannot cover every top-up plus fees.  Ceilings are REFUSED, never clamped.
//!
//! # Once per recipient
//!
//! Each target has the step id `A1.fund.<role-slug>`.  Before anything is sent for a target the write-ahead file
//! is read: a CONFIRMED record for that step id OR for that recipient pubkey (under any role or intent) means
//! `already_funded_by_record`: nothing is sent, whatever a lagging node reports, and even if the plan's target or
//! the role's pubkey changed since.  An OPEN or HELD record for the role or the recipient stops the whole command.
//! Only then is `top_up = target - balance` computed from a fresh read; a balance already at the target is
//! `noop_already_at_target` (the balance is never the only duplicate protection: the record check runs first).
//! Every send is one `Ctx::run_step` (size check, simulate, write-ahead BEFORE send, history-searching
//! unknown-outcome handling).  A held or rejected send stops the batch; a held record clears only when a later
//! run's history lookup finds the transaction (this tool has no command that clears one).
//!
//! # Solana notes (closest ICP / EVM comparisons)
//!
//! * A system wallet is an account with no data.  It exists only while it holds the rent-exempt minimum (890,880
//!   lamports for 0 bytes): a transfer that would leave a non-empty wallet below it is rejected, so a target under
//!   the floor can never be created, and the payer must keep the floor itself.  EVM has no such floor (any balance
//!   is fine); ICP's analogue is a canister's freezing threshold.
//! * The fee is per SIGNATURE (5,000 lamports for one signer), paid by the fee payer on top of the amount: EVM gas
//!   is per computation and paid in the same unit, here the bucket accounting adds the fee per transfer so the
//!   plan total is what actually leaves the deployer.
//! * Recipients need no key to receive: a transfer credits any address.  Which key controls an address is why the
//!   plan is pinned to the committed inventory (a wrong pubkey would silently pay a stranger).

use std::collections::BTreeMap;

use serde::de::{self, MapAccess, SeqAccess, Visitor};

use super::*;

// ---------------------------------------------------------------------------
// The packet's envelope, fixed in code (a plan may claim LESS, never more)
// ---------------------------------------------------------------------------

/// Packet 4c: one transfer between devnet keys is at most 2 SOL.
pub const FUND_PER_TRANSFER_CAP_LAMPORTS: u64 = 2_000_000_000;

/// Packet 4b bucket caps (lamports): Squads 0.5 SOL, Sponsor 1.0, Keeper 0.1, test wallets 7.0.
pub const FUND_BUCKET_CAPS: &[(&str, u64)] = &[
    ("squads", 500_000_000),
    ("sponsor", 1_000_000_000),
    ("keeper", 100_000_000),
    ("test_wallets", 7_000_000_000),
];

/// The only roles this command may fund, each with the one bucket it is charged to.
pub const FUND_ROLES: &[(&str, &str)] = &[
    ("G voter 1", "squads"),
    ("G voter 2", "squads"),
    ("G voter 3", "squads"),
    ("P voter 1", "squads"),
    ("P voter 2", "squads"),
    ("P voter 3", "squads"),
    ("plan wallet S", "sponsor"),
    ("management recipient", "sponsor"),
    ("plan wallet K", "keeper"),
    ("plan wallet A1", "test_wallets"),
    ("plan wallet A2", "test_wallets"),
    ("plan wallet P1", "test_wallets"),
    ("plan wallet P2", "test_wallets"),
    ("plan wallet P3", "test_wallets"),
    ("plan wallet P4", "test_wallets"),
    ("plan wallet P5", "test_wallets"),
    ("plan wallet W", "test_wallets"),
];

/// The inventory role of the payer.
const PAYER_ROLE: &str = "deployer D";
/// Plan and inventory files are tiny JSON documents.
const MAX_PLAN_FILE_BYTES: u64 = 256 * 1024;

fn role_entry(role: &str) -> Option<(&'static str, &'static str)> {
    FUND_ROLES.iter().find(|(name, _)| *name == role).copied()
}

fn bucket_entry(name: &str) -> Option<(&'static str, u64)> {
    FUND_BUCKET_CAPS.iter().find(|(bucket, _)| *bucket == name).copied()
}

/// `G voter 1` -> `g-voter-1`.  Only ever applied to an allowlisted role.
fn role_slug(role: &str) -> String {
    role.to_ascii_lowercase().replace(' ', "-")
}

fn step_id(role: &str) -> String {
    format!("A1.fund.{}", role_slug(role))
}

/// The semantic intent of one funding: `sha256(role | recipient | target_lamports | payer | genesis)`.
fn intent_sha256(role: &str, recipient: &Pubkey, target: u64, payer: &Pubkey, genesis: &str) -> String {
    devnet::sha256_hex(format!("{role}|{recipient}|{target}|{payer}|{genesis}").as_bytes())
}

// ---------------------------------------------------------------------------
// JSON without silent surprises
// ---------------------------------------------------------------------------

/// A JSON value parsed with DUPLICATE OBJECT KEYS REFUSED (`serde_json::Value` keeps the last one silently, so
/// two readers of the same file could disagree about what a plan says).
struct Strict(Value);

impl<'de> serde::Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Reader;
        impl<'de> Visitor<'de> for Reader {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Strict, E> {
                Ok(Strict(Value::from(v)))
            }
            fn visit_str<E>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(Value::String(v.to_string())))
            }
            fn visit_unit<E>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
                let mut items = Vec::new();
                while let Some(Strict(item)) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Strict(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
                let mut object = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if object.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate key `{key}`")));
                    }
                    let Strict(value) = map.next_value()?;
                    object.insert(key, value);
                }
                Ok(Strict(Value::Object(object)))
            }
        }
        deserializer.deserialize_any(Reader)
    }
}

fn parse_strict(bytes: &[u8], what: &str) -> Result<Value, OpsError> {
    serde_json::from_slice::<Strict>(bytes)
        .map(|Strict(value)| value)
        .map_err(|error| OpsError::Refused(format!("the {what} is not strict JSON ({error}); nothing was sent")))
}

fn j_obj<'v>(value: &'v Value, at: &str) -> Result<&'v serde_json::Map<String, Value>, OpsError> {
    value
        .as_object()
        .ok_or_else(|| OpsError::Refused(format!("{at} must be a JSON object; nothing was sent")))
}

/// Refuse any key the tool does not understand: a silently ignored field is a plan nobody reviewed.
fn j_only(map: &serde_json::Map<String, Value>, allowed: &[&str], at: &str) -> Result<(), OpsError> {
    match map.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => refuse(format!("{at} has an unknown key `{key}` (allowed: {}); nothing was sent", allowed.join(", "))),
        None => Ok(()),
    }
}

fn j_get<'v>(map: &'v serde_json::Map<String, Value>, key: &str, at: &str) -> Result<&'v Value, OpsError> {
    map.get(key)
        .ok_or_else(|| OpsError::Refused(format!("{at} is missing `{key}`; nothing was sent")))
}

fn j_text<'v>(map: &'v serde_json::Map<String, Value>, key: &str, at: &str) -> Result<&'v str, OpsError> {
    j_get(map, key, at)?
        .as_str()
        .ok_or_else(|| OpsError::Refused(format!("{at}.{key} must be a string; nothing was sent")))
}

/// A non-negative integer number of lamports (a negative, fractional or oversized number is refused).
fn j_lamports(map: &serde_json::Map<String, Value>, key: &str, at: &str) -> Result<u64, OpsError> {
    let value = j_get(map, key, at)?;
    value.as_u64().ok_or_else(|| {
        OpsError::Refused(format!(
            "{at}.{key} must be a non-negative integer number of lamports (got {value}); nothing was sent"
        ))
    })
}

fn j_pubkey(map: &serde_json::Map<String, Value>, key: &str, at: &str) -> Result<Pubkey, OpsError> {
    let text = j_text(map, key, at)?;
    Pubkey::from_str(text).map_err(|_| OpsError::Refused(format!("{at}.{key} `{text}` is not a base58 pubkey; nothing was sent")))
}

fn overflow<T>(what: &str) -> Result<T, OpsError> {
    refuse(format!("arithmetic overflow while adding up {what}; the plan is refused, nothing was sent"))
}

// ---------------------------------------------------------------------------
// Plan and inventory files, pinned to a commit
// ---------------------------------------------------------------------------

/// A file that was read (and, with `--plan-commit`, proven equal to its committed blob).
struct PinnedFile {
    bytes: Vec<u8>,
    path: String,
    commit: Option<String>,
    sha256: String,
}

impl PinnedFile {
    fn to_json(&self) -> Value {
        json!({ "path": self.path, "commit": self.commit, "sha256": self.sha256 })
    }
}

/// A path inside the repository: relative, normal components only, no hidden component, conservative characters
/// (it becomes part of a `git cat-file <commit>:<path>` argument).
fn repo_relative(spec: &str, what: &str) -> Result<String, OpsError> {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    if spec.is_empty()
        || spec.starts_with('/')
        || !spec.chars().all(plain)
        || spec.split('/').any(|part| part.is_empty() || part.starts_with('.'))
    {
        return refuse(format!(
            "{what} `{spec}` is not a plain repository-relative path (letters, digits, `.`, `_`, `-`, `/`; no `..`, no \
             hidden component, no leading or trailing `/`)"
        ));
    }
    Ok(spec.to_string())
}

/// Where `--plan` points: `(path recorded in receipts, file to read)`.  A relative path is relative to the
/// repository root (so it means the same wherever the operator stands); an absolute one must lie inside the
/// repository when it is pinned.
fn locate_plan(repo: &Path, spec: &str, pinned: bool) -> Result<(String, PathBuf), OpsError> {
    refuse_default_wallet(Path::new(spec))?;
    let given = Path::new(spec);
    if !given.is_absolute() {
        let rel = repo_relative(spec, "--plan")?;
        let abs = repo.join(&rel);
        return Ok((rel, abs));
    }
    let resolve = |path: &Path| {
        path.canonicalize()
            .map_err(|error| OpsError::Refused(format!("cannot resolve {}: {error}", path.display())))
    };
    let canonical = resolve(given)?;
    let root = resolve(repo)?;
    match canonical.strip_prefix(&root) {
        Ok(rel) => {
            let rel = rel
                .to_str()
                .ok_or_else(|| OpsError::Refused("--plan is not a UTF-8 path".into()))?;
            Ok((repo_relative(rel, "--plan")?, given.to_path_buf()))
        }
        Err(_) if !pinned => Ok((spec.to_string(), given.to_path_buf())),
        Err(_) => refuse(format!(
            "--plan {spec} is outside the repository {}; a plan pinned by --plan-commit is a path inside it",
            root.display()
        )),
    }
}

/// A pinned file must BE the committed path, not a symlink to something else.
fn ensure_plain_path(repo: &Path, rel: &str, abs: &Path, what: &str) -> Result<(), OpsError> {
    let resolve = |path: &Path| {
        path.canonicalize()
            .map_err(|error| OpsError::Refused(format!("cannot resolve the {what} {}: {error}", path.display())))
    };
    let want = resolve(repo)?.join(rel);
    let got = resolve(abs)?;
    if want != got {
        return refuse(format!(
            "the {what} {rel} resolves to {} (a symlink?); a pinned file must be the committed path itself",
            got.display()
        ));
    }
    Ok(())
}

/// `git cat-file blob <commit>:<rel>` must equal `working` byte for byte.
fn verify_blob(repo: &Path, commit: &str, rel: &str, working: &[u8], what: &str) -> Result<(), OpsError> {
    if !is_lower_hex(commit, 40) {
        return refuse("--plan-commit must be a full 40-digit lowercase hex commit id");
    }
    let kind = git_stdout(repo, &["cat-file", "-t", commit])?;
    if String::from_utf8_lossy(&kind).trim() != "commit" {
        return refuse(format!("--plan-commit {commit} is not a commit object in {}", repo.display()));
    }
    let object = format!("{commit}:{rel}");
    let kind = git_stdout(repo, &["cat-file", "-t", &object])?;
    if String::from_utf8_lossy(&kind).trim() != "blob" {
        return refuse(format!("{rel} at {commit} is not a file"));
    }
    let blob = git_stdout(repo, &["cat-file", "blob", &object])?;
    if blob != working {
        return refuse(format!(
            "the working {what} {rel} differs from the blob committed at {commit}; commit (freeze) it and pass that \
             commit, or restore the file"
        ));
    }
    Ok(())
}

fn read_pinned(repo: &Path, rel: &str, abs: &Path, commit: Option<&str>, what: &str) -> Result<PinnedFile, OpsError> {
    if commit.is_some() {
        ensure_plain_path(repo, rel, abs, what)?;
    }
    let metadata = fs::metadata(abs).map_err(|error| OpsError::Refused(format!("cannot read the {what} {}: {error}", abs.display())))?;
    if !metadata.is_file() || metadata.len() > MAX_PLAN_FILE_BYTES {
        return refuse(format!(
            "the {what} {} is not a regular file of at most {MAX_PLAN_FILE_BYTES} bytes",
            abs.display()
        ));
    }
    let bytes = fs::read(abs).map_err(|error| OpsError::Refused(format!("cannot read the {what} {}: {error}", abs.display())))?;
    if std::str::from_utf8(&bytes).is_ok_and(looks_like_keypair) {
        return refuse(format!("the {what} {} looks like a keypair file; refusing to read it", abs.display()));
    }
    if let Some(commit) = commit {
        verify_blob(repo, commit, rel, &bytes, what)?;
    }
    Ok(PinnedFile {
        sha256: devnet::sha256_hex(&bytes),
        bytes,
        path: rel.to_string(),
        commit: commit.map(str::to_string),
    })
}

// ---------------------------------------------------------------------------
// Whole-plan validation (pure: no network, no clock)
// ---------------------------------------------------------------------------

struct FundTarget {
    role: &'static str,
    step: String,
    pubkey: Pubkey,
    bucket: &'static str,
    target: u64,
    intent: String,
}

struct BucketPlan {
    name: &'static str,
    cap: u64,
    already_spent: u64,
    targets_sum: u64,
    transfers: u64,
    fees: u64,
    total: u64,
}

impl BucketPlan {
    fn to_json(&self) -> Value {
        json!({
            "bucket": self.name, "cap_lamports": self.cap, "already_spent_lamports": self.already_spent,
            "targets_lamports": self.targets_sum, "transfers": self.transfers, "fees_lamports": self.fees,
            "total_lamports": self.total, "headroom_lamports": self.cap - self.total,
        })
    }
    /// What this command may still spend from the bucket (the plan's `already_spent` is the history).
    fn remaining(&self) -> u64 {
        self.cap - self.already_spent
    }
}

struct ValidPlan {
    targets: Vec<FundTarget>,
    buckets: Vec<BucketPlan>,
    fee: u64,
    command_total: u64,
    caps_sum: u64,
}

/// `keys.public_keys` of the inventory file: role -> pubkey.
fn inventory_keys(inventory: &Value) -> Result<BTreeMap<String, Pubkey>, OpsError> {
    let keys = inventory
        .get("keys")
        .and_then(|keys| keys.get("public_keys"))
        .and_then(Value::as_object)
        .ok_or_else(|| OpsError::Refused("the inventory has no `keys.public_keys` object; nothing was sent".into()))?;
    keys.iter()
        .map(|(role, value)| {
            let text = value
                .as_str()
                .ok_or_else(|| OpsError::Refused(format!("inventory `{role}` is not a string; nothing was sent")))?;
            let key = Pubkey::from_str(text)
                .map_err(|_| OpsError::Refused(format!("inventory `{role}` `{text}` is not a base58 pubkey; nothing was sent")))?;
            Ok((role.clone(), key))
        })
        .collect()
}

struct BucketSpec {
    claimed_cap: u64,
    already_spent: u64,
    planned_total: Option<u64>,
}

/// Everything about the plan that needs no network.  The first problem refuses the WHOLE command.
fn validate_plan(
    plan: &Value,
    inventory: &Value,
    payer: &Pubkey,
    cluster: Target,
    genesis: &str,
) -> Result<ValidPlan, OpsError> {
    let root = j_obj(plan, "the plan")?;
    j_only(
        root,
        &[
            "schema",
            "purpose",
            "network",
            "genesis",
            "payer",
            "inventory",
            "per_transfer_cap_lamports",
            "fee_per_transfer_lamports",
            "buckets",
            "targets",
            "not_in_this_plan",
            "totals",
        ],
        "plan",
    )?;
    if j_get(root, "schema", "plan")?.as_u64() != Some(1) {
        return refuse("plan: `schema` must be 1; nothing was sent");
    }
    let want_network = match cluster {
        Target::Local => "local",
        Target::Devnet => "devnet",
    };
    let network = j_text(root, "network", "plan")?;
    if network != want_network {
        return refuse(format!("plan: `network` is `{network}` but this run targets `{want_network}`; nothing was sent"));
    }
    let plan_genesis = j_text(root, "genesis", "plan")?;
    if plan_genesis != genesis {
        return refuse(format!(
            "plan: `genesis` {plan_genesis} is not the cluster's genesis hash {genesis}; the plan was written for another cluster"
        ));
    }
    let payer_obj = j_obj(j_get(root, "payer", "plan")?, "plan.payer")?;
    j_only(payer_obj, &["role", "pubkey"], "plan.payer")?;
    if j_text(payer_obj, "role", "plan.payer")? != PAYER_ROLE {
        return refuse(format!("plan.payer.role must be `{PAYER_ROLE}`; nothing was sent"));
    }
    let plan_payer = j_pubkey(payer_obj, "pubkey", "plan.payer")?;
    if plan_payer != *payer {
        return refuse(format!("plan.payer.pubkey {plan_payer} is not the --payer key {payer}; nothing was sent"));
    }
    let keys = inventory_keys(inventory)?;
    if keys.get(PAYER_ROLE) != Some(payer) {
        return refuse(format!("the inventory's `{PAYER_ROLE}` is not the --payer key {payer}; nothing was sent"));
    }

    // The ceilings the plan claims: fixed by the packet, refused when higher, never clamped.
    let per_transfer_cap = j_lamports(root, "per_transfer_cap_lamports", "plan")?;
    if per_transfer_cap > FUND_PER_TRANSFER_CAP_LAMPORTS {
        return refuse(format!(
            "plan claims a per-transfer cap of {per_transfer_cap} lamports, above the packet's {FUND_PER_TRANSFER_CAP_LAMPORTS} \
             (4c: 2 SOL); refused, never clamped"
        ));
    }
    if per_transfer_cap != FUND_PER_TRANSFER_CAP_LAMPORTS {
        return refuse(format!(
            "plan.per_transfer_cap_lamports must be exactly the packet's {FUND_PER_TRANSFER_CAP_LAMPORTS} (got {per_transfer_cap})"
        ));
    }
    let fee = j_lamports(root, "fee_per_transfer_lamports", "plan")?;
    if fee != SIGNATURE_FEE_LAMPORTS {
        return refuse(format!(
            "plan.fee_per_transfer_lamports must be exactly the {SIGNATURE_FEE_LAMPORTS}-lamport signature fee (got {fee})"
        ));
    }

    let mut declared: BTreeMap<&'static str, BucketSpec> = BTreeMap::new();
    for (name, value) in j_obj(j_get(root, "buckets", "plan")?, "plan.buckets")? {
        let Some((known, code_cap)) = bucket_entry(name) else {
            return refuse(format!(
                "plan.buckets has an unknown bucket `{name}` (known: {}); nothing was sent",
                FUND_BUCKET_CAPS.iter().map(|(bucket, _)| *bucket).collect::<Vec<_>>().join(", ")
            ));
        };
        let at = format!("plan.buckets.{name}");
        let bucket = j_obj(value, &at)?;
        j_only(bucket, &["cap_lamports", "already_spent_lamports", "history", "planned_total_with_fees_lamports"], &at)?;
        let claimed_cap = j_lamports(bucket, "cap_lamports", &at)?;
        if claimed_cap > code_cap {
            return refuse(format!(
                "bucket `{name}` claims a cap of {claimed_cap} lamports, larger than the packet's {code_cap} (4b); refused, never clamped"
            ));
        }
        if bucket.get("history").is_some_and(|history| !history.is_array()) {
            return refuse(format!("{at}.history must be an array; nothing was sent"));
        }
        let planned_total = match bucket.get("planned_total_with_fees_lamports") {
            None | Some(Value::Null) => None,
            Some(_) => Some(j_lamports(bucket, "planned_total_with_fees_lamports", &at)?),
        };
        declared.insert(
            known,
            BucketSpec {
                claimed_cap,
                already_spent: j_lamports(bucket, "already_spent_lamports", &at)?,
                planned_total,
            },
        );
    }

    let rows = j_get(root, "targets", "plan")?
        .as_array()
        .ok_or_else(|| OpsError::Refused("plan.targets must be an array; nothing was sent".into()))?;
    if rows.is_empty() {
        return refuse("plan.targets is empty; nothing to fund");
    }
    let mut targets: Vec<FundTarget> = Vec::with_capacity(rows.len());
    let mut seen_roles: BTreeSet<&'static str> = BTreeSet::new();
    let mut seen_keys: BTreeSet<Pubkey> = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let at = format!("plan.targets[{index}]");
        let row = j_obj(row, &at)?;
        j_only(row, &["role", "pubkey", "bucket", "target_lamports"], &at)?;
        let role_name = j_text(row, "role", &at)?;
        let Some((role, role_bucket)) = role_entry(role_name) else {
            return refuse(format!(
                "{at}: role `{role_name}` is not one of the {} plan roles this command may fund; nothing was sent",
                FUND_ROLES.len()
            ));
        };
        if !seen_roles.insert(role) {
            return refuse(format!("{at}: role `{role}` appears twice in the plan; nothing was sent"));
        }
        let pubkey = j_pubkey(row, "pubkey", &at)?;
        if !seen_keys.insert(pubkey) {
            return refuse(format!("{at}: pubkey {pubkey} (`{role}`) appears twice in the plan; nothing was sent"));
        }
        if pubkey == *payer {
            return refuse(format!("{at}: `{role}` is the payer itself ({pubkey}); a self-transfer funds nothing"));
        }
        if pubkey == Pubkey::default() {
            return refuse(format!("{at}: `{role}` is the all-zero pubkey"));
        }
        match keys.get(role) {
            Some(inventory_key) if *inventory_key == pubkey => {}
            Some(inventory_key) => {
                return refuse(format!(
                    "{at}: `{role}` is {pubkey} in the plan but {inventory_key} in the inventory; no aliases, nothing was sent"
                ))
            }
            None => return refuse(format!("{at}: `{role}` is not in the inventory; nothing was sent")),
        }
        let bucket_name = j_text(row, "bucket", &at)?;
        let Some((bucket, _)) = bucket_entry(bucket_name) else {
            return refuse(format!("{at}: unknown bucket `{bucket_name}`; nothing was sent"));
        };
        if bucket != role_bucket {
            return refuse(format!("{at}: `{role}` is charged to bucket `{role_bucket}`, not `{bucket}`; nothing was sent"));
        }
        if !declared.contains_key(bucket) {
            return refuse(format!("{at}: bucket `{bucket}` is not declared in plan.buckets; nothing was sent"));
        }
        let target = j_lamports(row, "target_lamports", &at)?;
        if target == 0 {
            return refuse(format!("{at}: `{role}` has a target of zero lamports; nothing was sent"));
        }
        if target > FUND_PER_TRANSFER_CAP_LAMPORTS {
            return refuse(format!(
                "{at}: `{role}` target {target} lamports is above the per-transfer cap {FUND_PER_TRANSFER_CAP_LAMPORTS} (4c); \
                 refused, never clamped"
            ));
        }
        targets.push(FundTarget {
            role,
            step: step_id(role),
            pubkey,
            bucket,
            target,
            intent: intent_sha256(role, &pubkey, target, payer, genesis),
        });
    }

    // The command's own total against the sum of the packet's bucket caps ...
    let transfers = targets.len() as u64;
    let mut command_total: u64 = 0;
    for target in &targets {
        command_total = match command_total.checked_add(target.target).and_then(|sum| sum.checked_add(fee)) {
            Some(sum) => sum,
            None => return overflow("the command's total"),
        };
    }
    let mut caps_sum: u64 = 0;
    for (_, cap) in FUND_BUCKET_CAPS {
        caps_sum = match caps_sum.checked_add(*cap) {
            Some(sum) => sum,
            None => return overflow("the bucket caps"),
        };
    }
    if command_total > caps_sum {
        return refuse(format!(
            "the command's total {command_total} lamports ({transfers} targets plus {fee} lamports fee each) is above the sum \
             of the packet's bucket caps {caps_sum} (4b); refused, never clamped"
        ));
    }

    // ... and each bucket: history already spent + targets + fees, against the cap (the plan's, which is at most the packet's).
    let mut buckets = Vec::with_capacity(declared.len());
    for (name, spec) in &declared {
        let in_bucket: Vec<&FundTarget> = targets.iter().filter(|target| target.bucket == *name).collect();
        let count = in_bucket.len() as u64;
        let mut targets_sum: u64 = 0;
        for target in &in_bucket {
            targets_sum = match targets_sum.checked_add(target.target) {
                Some(sum) => sum,
                None => return overflow(&format!("bucket `{name}`")),
            };
        }
        let Some(fees) = count.checked_mul(fee) else { return overflow(&format!("bucket `{name}` fees")) };
        let Some(total) = spec.already_spent.checked_add(targets_sum).and_then(|sum| sum.checked_add(fees)) else {
            return overflow(&format!("bucket `{name}`"));
        };
        if total > spec.claimed_cap {
            return refuse(format!(
                "bucket `{name}`: already spent {} + targets {targets_sum} + {count} x {fee} fees = {total} lamports is above \
                 its cap {} (4b); refused, never clamped. No top-up without a new approval",
                spec.already_spent, spec.claimed_cap
            ));
        }
        if let Some(claimed) = spec.planned_total {
            if claimed != total {
                return refuse(format!(
                    "bucket `{name}`: the plan's planned_total_with_fees_lamports {claimed} is not the computed {total}; \
                     the plan is inconsistent"
                ));
            }
        }
        buckets.push(BucketPlan {
            name,
            cap: spec.claimed_cap,
            already_spent: spec.already_spent,
            targets_sum,
            transfers: count,
            fees,
            total,
        });
    }
    if let Some(totals) = root.get("totals").filter(|totals| !totals.is_null()) {
        let totals = j_obj(totals, "plan.totals")?;
        j_only(totals, &["targets_lamports", "fees_lamports"], "plan.totals")?;
        let Some(computed_targets) = targets.iter().try_fold(0u64, |sum, target| sum.checked_add(target.target)) else {
            return overflow("the targets");
        };
        let Some(computed_fees) = transfers.checked_mul(fee) else { return overflow("the fees") };
        if j_lamports(totals, "targets_lamports", "plan.totals")? != computed_targets
            || j_lamports(totals, "fees_lamports", "plan.totals")? != computed_fees
        {
            return refuse(format!(
                "plan.totals disagrees with the targets (computed targets {computed_targets}, fees {computed_fees}); the plan is inconsistent"
            ));
        }
    }
    Ok(ValidPlan {
        targets,
        buckets,
        fee,
        command_total,
        caps_sum,
    })
}

// ---------------------------------------------------------------------------
// The per-target decision
// ---------------------------------------------------------------------------

/// A CONFIRMED write-ahead record that already covers a target.
#[derive(Clone)]
struct RecordHit {
    signature: String,
    step: String,
    slot: Option<u64>,
    recipient: Option<String>,
    by_role: bool,
    by_recipient: bool,
    same_intent: Option<bool>,
}

#[derive(Clone)]
enum Eval {
    /// Never sent again: the write-ahead file holds a confirmed record for the role or the recipient.
    Record(RecordHit),
    /// No record, and the recipient already holds its target.
    AtTarget { balance: u64 },
    /// No record: `top_up = target - balance`.
    TopUp { balance: u64, top_up: u64 },
}

/// What the write-ahead file and a fresh read say about one target.  Refuses (stopping the whole command) on an
/// OPEN or HELD record for the role or the recipient, and on a recipient that is not a plain system wallet.
fn evaluate(ctx: &Ctx<'_>, target: &FundTarget) -> Result<Eval, OpsError> {
    let recipient = target.pubkey.to_string();
    if let Some(path) = &ctx.pending_path {
        let log = read_pending(path, &ctx.genesis)?;
        let matches_role = |step: &str| step == target.step;
        let matches_recipient = |other: &Option<String>| other.as_deref() == Some(recipient.as_str());
        if let Some(open) = log
            .open
            .iter()
            .find(|record| matches_role(&record.step) || matches_recipient(&record.recipient))
        {
            return refuse(format!(
                "`{}` ({recipient}) has an OPEN{} write-ahead record (signature {}, step {}, recorded in {}): the outcome of \
                 that transfer is not known, so NOTHING further is sent by this command. A re-run re-queries it; the tool has no \
                 command that clears an unknown record",
                target.role,
                if open.held.is_some() { " (HELD UNKNOWN)" } else { "" },
                open.signature,
                open.step,
                path.display()
            ));
        }
        if let Some(done) = log
            .confirmed
            .iter()
            .find(|record| matches_role(&record.step) || matches_recipient(&record.recipient))
        {
            return Ok(Eval::Record(RecordHit {
                signature: done.signature.clone(),
                step: done.step.clone(),
                slot: done.slot,
                recipient: done.recipient.clone(),
                by_role: matches_role(&done.step),
                by_recipient: matches_recipient(&done.recipient),
                same_intent: done.intent.as_deref().map(|intent| intent == target.intent),
            }));
        }
    }
    let account = ctx.account(&target.pubkey)?;
    if let Some(account) = &account {
        if account.owner != system_program::ID || !account.data.is_empty() || account.executable {
            return refuse(format!(
                "`{}` ({recipient}) is not a plain system wallet (owner {}, {} data bytes); nothing was sent",
                target.role,
                account.owner,
                account.data.len()
            ));
        }
    }
    let balance = account.map_or(0, |account| account.lamports);
    Ok(if balance >= target.target {
        Eval::AtTarget { balance }
    } else {
        Eval::TopUp {
            balance,
            top_up: target.target - balance,
        }
    })
}

#[derive(Default)]
struct Report {
    rows: Vec<Value>,
    steps: Vec<Value>,
    checks: Vec<Value>,
    funded: u64,
    by_record: u64,
    at_target: u64,
    would_fund: u64,
    lamports_sent: u64,
}

fn record_json(hit: &RecordHit) -> Value {
    json!({
        "signature": hit.signature, "step": hit.step, "slot": hit.slot, "recipient": hit.recipient,
        "matched_by_role": hit.by_role, "matched_by_recipient": hit.by_recipient, "same_intent": hit.same_intent,
    })
}

/// Process the targets in plan order.  `planned` is the dry-run / planning pass; an `--execute` run re-reads the
/// write-ahead file and the balance of each target immediately before it.
fn run_batch(
    ctx: &Ctx<'_>,
    valid: &ValidPlan,
    payer: &KeyFile,
    planned: &[Eval],
    pins: &Value,
    report: &mut Report,
) -> Result<(), OpsError> {
    let mut spent: BTreeMap<&'static str, u64> = BTreeMap::new();
    for (index, target) in valid.targets.iter().enumerate() {
        let eval = if ctx.executing() {
            evaluate(ctx, target)?
        } else {
            planned[index].clone()
        };
        let mut row = json!({
            "role": target.role, "step": target.step, "recipient": target.pubkey.to_string(), "bucket": target.bucket,
            "target_lamports": target.target, "intent_sha256": target.intent,
        });
        match eval {
            Eval::Record(hit) => {
                progress(format!(
                    "{}: already funded by a confirmed record ({}); nothing sent",
                    target.step, hit.signature
                ));
                row["outcome"] = json!("already_funded_by_record");
                row["record"] = record_json(&hit);
                report.by_record += 1;
            }
            Eval::AtTarget { balance } => {
                progress(format!("{}: balance {balance} is already at the target; nothing sent", target.step));
                row["outcome"] = json!("noop_already_at_target");
                row["balance_before"] = json!(balance);
                report.at_target += 1;
            }
            Eval::TopUp { balance, top_up } => {
                // Defence in depth: the validation already guarantees all of this.
                if top_up == 0 || top_up > FUND_PER_TRANSFER_CAP_LAMPORTS || top_up > target.target {
                    return refuse(format!("{}: top-up {top_up} is outside 1..={FUND_PER_TRANSFER_CAP_LAMPORTS}", target.step));
                }
                let bucket = valid
                    .buckets
                    .iter()
                    .find(|bucket| bucket.name == target.bucket)
                    .ok_or_else(|| OpsError::Failed(format!("internal: bucket {} vanished", target.bucket)))?;
                let used = spent.entry(target.bucket).or_insert(0);
                let next = used
                    .checked_add(top_up)
                    .and_then(|sum| sum.checked_add(valid.fee))
                    .filter(|sum| *sum <= bucket.remaining());
                let Some(next) = next else {
                    return refuse(format!(
                        "{}: this command would spend more than bucket `{}` has left ({} lamports); stopped",
                        target.step,
                        target.bucket,
                        bucket.remaining()
                    ));
                };
                let step = Step {
                    id: target.step.clone(),
                    label: format!("system transfer {top_up} lamports to {} ({})", target.role, target.pubkey),
                    instructions: vec![system_instruction::transfer(&payer.pubkey, &target.pubkey, top_up)],
                    payer: payer.pubkey,
                    cu_limit: None,
                    depends_on_previous: false,
                };
                let annotation = json!({
                    "role": target.role, "role_slug": role_slug(target.role), "recipient": target.pubkey.to_string(),
                    "bucket": target.bucket, "target_lamports": target.target, "top_up_lamports": top_up,
                    "intent_sha256": target.intent, "plan": pins["plan"], "inventory": pins["inventory"],
                });
                let signers: Vec<&Keypair> = if ctx.executing() { vec![payer.keypair()?] } else { Vec::new() };
                let result = ctx.run_step_annotated(&step, &signers, Some(&annotation))?;
                row["balance_before"] = json!(balance);
                row["top_up_lamports"] = json!(top_up);
                if result.executed {
                    *used = next;
                    let after = ctx.balance(&target.pubkey)?;
                    report.checks.push(check(
                        format!("{} balance after >= target", target.role),
                        after >= target.target,
                        target.target,
                        after,
                    ));
                    row["outcome"] = json!("funded");
                    row["balance_after"] = json!(after);
                    report.funded += 1;
                    report.lamports_sent = report.lamports_sent.saturating_add(top_up);
                } else {
                    row["outcome"] = json!("would_fund");
                    report.would_fund += 1;
                }
                report.steps.push(result.to_json());
            }
        }
        report.rows.push(row);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------

pub(super) fn cmd_fund_wallets(ctx: &Ctx<'_>, repo: &Path) -> Result<Value, OpsError> {
    let opts = ctx.opts;
    let payer = key_flag(opts, "payer")?;
    let plan_spec = opts.require("plan")?;
    let plan_commit = opts.get("plan-commit");
    let executing = ctx.executing();
    if executing && ctx.gate.target == Target::Devnet && plan_commit.is_none() {
        return refuse(
            "--execute on devnet needs --plan-commit <40-hex commit>: the plan file and the inventory file it names must \
             each equal their blob at that commit (an artifact-identity pin; NOT an approval)",
        );
    }
    if executing && ctx.pending_path.is_none() {
        return refuse(
            "fund-wallets --execute needs --pending-file <path.jsonl>: the once-per-recipient guard reads the write-ahead \
             file, and a balance read alone must never be the only protection against paying twice",
        );
    }

    // The plan and its inventory, each equal to its committed blob when pinned.
    let pinned = plan_commit.is_some();
    let (plan_rel, plan_abs) = locate_plan(repo, plan_spec, pinned)?;
    let plan_file = read_pinned(repo, &plan_rel, &plan_abs, plan_commit, "plan")?;
    let plan_value = parse_strict(&plan_file.bytes, "plan")?;
    let inventory_spec = j_text(j_obj(&plan_value, "the plan")?, "inventory", "plan")?;
    let inventory_rel = repo_relative(inventory_spec, "plan.inventory")?;
    refuse_default_wallet(Path::new(&inventory_rel))?;
    let inventory_file = read_pinned(repo, &inventory_rel, &repo.join(&inventory_rel), plan_commit, "inventory")?;
    let inventory_value = parse_strict(&inventory_file.bytes, "inventory")?;
    let pins = json!({ "plan": plan_file.to_json(), "inventory": inventory_file.to_json() });

    // Whole-plan validation: any failure refuses the command with zero sends.
    let valid = validate_plan(&plan_value, &inventory_value, &payer.pubkey, ctx.gate.target, &ctx.genesis)?;

    // What the cluster says (reads carry the seeded minContextSlot): rent floor, each target, the payer.
    let rent_floor = ctx.rent(0)?;
    for target in &valid.targets {
        if target.target < rent_floor {
            return refuse(format!(
                "`{}` target {} is below the {rent_floor}-lamport rent-exempt minimum of an empty wallet, so the account \
                 could not be created; nothing was sent",
                target.role, target.target
            ));
        }
    }
    let payer_before = ctx.balance(&payer.pubkey)?;
    let mut planned = Vec::with_capacity(valid.targets.len());
    let mut planned_top_ups: u64 = 0;
    let mut planned_sends: u64 = 0;
    for target in &valid.targets {
        let eval = evaluate(ctx, target)?;
        if let Eval::TopUp { top_up, .. } = &eval {
            planned_top_ups = match planned_top_ups.checked_add(*top_up) {
                Some(sum) => sum,
                None => return overflow("the planned top-ups"),
            };
            planned_sends += 1; // at most one per allowlisted role: 17
        }
        planned.push(eval);
    }
    // The payer must cover every top-up and fee AND keep the rent floor itself (a wallet left between 0 and the
    // floor is rejected by the runtime, which would fail the LAST transfers after the first ones were sent).
    let Some(fees) = planned_sends.checked_mul(valid.fee) else { return overflow("the planned fees") };
    let keep = if planned_sends > 0 { rent_floor } else { 0 };
    let required = match planned_top_ups.checked_add(fees).and_then(|sum| sum.checked_add(keep)) {
        Some(sum) => sum,
        None => return overflow("the payer's requirement"),
    };
    if payer_before < required {
        return refuse(format!(
            "the payer {} holds {payer_before} lamports but needs {required} ({planned_top_ups} of top-ups, {fees} of fees and \
             the {keep}-lamport rent floor it must keep) to fund {planned_sends} target(s); nothing was sent",
            payer.pubkey
        ));
    }

    let mut report = Report::default();
    if let Err(error) = run_batch(ctx, &valid, &payer, &planned, &pins, &mut report) {
        // The batch stopped (a held or rejected send, an open record, a refusal): the funds that did move are in the
        // write-ahead file; print what this run did so the operator does not have to reconstruct it.
        progress(format!(
            "fund-wallets STOPPED: {error}\n[devnet_ops] fund-wallets targets handled before the stop: {}",
            serde_json::to_string(&report.rows).unwrap_or_default()
        ));
        return Err(error);
    }
    let payer_after = ctx.balance(&payer.pubkey)?;
    let all_ok = checks_ok(&report.checks);
    Ok(json!({
        "command": "fund-wallets",
        "mode": ctx.mode(),
        "payer": payer.pubkey.to_string(),
        "genesis_hash": ctx.genesis,
        "plan": pins["plan"],
        "inventory": pins["inventory"],
        "envelope": {
            "per_transfer_cap_lamports": FUND_PER_TRANSFER_CAP_LAMPORTS,
            "fee_per_transfer_lamports": valid.fee,
            "command_total_lamports": valid.command_total,
            "sum_of_bucket_caps_lamports": valid.caps_sum,
            "rent_floor_lamports": rent_floor,
            "buckets": valid.buckets.iter().map(BucketPlan::to_json).collect::<Vec<_>>(),
        },
        "targets": report.rows,
        "totals": {
            "targets": valid.targets.len(),
            "funded": report.funded,
            "would_fund": report.would_fund,
            "already_funded_by_record": report.by_record,
            "noop_already_at_target": report.at_target,
            "lamports_sent": report.lamports_sent,
            "fees_lamports": report.funded.saturating_mul(valid.fee),
        },
        "payer_balance_before": payer_before,
        "payer_balance_after": payer_after,
        "steps": report.steps,
        "checks": report.checks,
        "all_ok": all_ok,
    }))
}
