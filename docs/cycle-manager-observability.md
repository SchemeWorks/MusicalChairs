# Cycle Manager Observability

Musical Chairs exposes Cycle Manager data through a project-owned observatory canister. Cycle Manager reads the observatory and first-party self-report methods; it is not a controller of Musical Chairs canisters.

## Architecture

Cycle Manager polls two surfaces:

1. `musical_chairs_observatory.cycle_manager_targets()` returns the static Musical Chairs target inventory.
2. First-party canisters expose `cycles_status()` and `cycle_manager_metrics()` directly.
3. Unmodifiable canisters are observed by `musical_chairs_observatory.collect_controlled_statuses()`, which stores latest-only snapshots that Cycle Manager reads with `controlled_statuses()`.

The observatory exists because `pp_ledger`, `siws_provider`, `frontend`, and `pp_assets` cannot be extended with Musical Chairs self-report APIs. The observatory can be added as their controller so it can call management-canister `canister_status`; Cycle Manager does not need that authority. The management-canister `canister_status` shape is documented in the [IC management canister reference](https://docs.internetcomputer.org/references/ic-interface-spec/management-canister/#ic-method-canister_status).

## Target Kinds

Self-report targets:

- `backend`
- `ponzi_math`
- `ponzi_math_sol`
- `shenanigans`

Controller-status targets:

- `pp_ledger`
- `siws_provider`
- `frontend`
- `pp_assets`

## Security Model

Cycle Manager must not be added as a controller of any Musical Chairs canister. The observatory is the only new controller, and only for fixed canisters that need management-canister status collection.

The observatory intentionally does not expose `install_code`, `stop_canister`, `delete_canister`, `update_settings`, `deposit_cycles`, or generic management-canister passthroughs. Its only management-canister call is `canister_status` over the static controller-status target set.

Admin methods reject anonymous callers and use a persistent allowlist. `remove_admin` refuses to remove the final admin. `collect_controlled_statuses()` is also admin-only because it exercises the observatory's controller authority.

Controller authority is still broad at the IC level. Treat observatory upgrades and controller setup as production-sensitive operations.

## Deploy

Build and create/deploy the observatory with the existing `dfx.json` project flow:

```bash
dfx build musical_chairs_observatory --check
dfx deploy --network ic musical_chairs_observatory
```

Before deployment, `cycle-manager.targets.json` uses `<musical_chairs_observatory.ic>` as a placeholder. After deployment, add the generated `musical_chairs_observatory.ic` ID to `canister_ids.json` and replace every `<musical_chairs_observatory.ic>` placeholder in `cycle-manager.targets.json` with the deployed observatory principal.

## Controller Setup

Add the observatory as an additional controller. Do not remove or replace existing controllers.

```bash
CONFIRM_MAINNET=1 DFX_NETWORK=ic scripts/setup-observatory-controllers.sh
```

By default, the script updates:

- `pp_ledger`
- `siws_provider`
- `frontend`
- `pp_assets`

Do not add the observatory as a controller of first-party canisters for the standard Cycle Manager path. `backend`, `ponzi_math`, `ponzi_math_sol`, and `shenanigans` self-report directly.

`expected_controllers` is a required-controller set for Cycle Manager audits, not an instruction to replace the full controller list. The setup script preserves existing controllers with `--add-controller`, and collected snapshots include the observed `controllers` list so Cycle Manager can display or audit the complete live set.

## Collection

Run collection from an observatory admin principal:

```bash
dfx canister --network ic call musical_chairs_observatory collect_controlled_statuses '()'
```

Cycle Manager should read:

```bash
dfx canister --network ic call musical_chairs_observatory cycle_manager_targets '()'
dfx canister --network ic call musical_chairs_observatory controlled_statuses '()'
```

If Cycle Manager needs to trigger collection itself, add only the Cycle Manager service principal to the observatory admin allowlist. Do not make Cycle Manager a controller of any Musical Chairs canister. If any controlled target fails during a collection run, the observatory stores successful snapshots but returns `#Err` with the partial collection count and last failure.

## Cycle Manager Adapter

If Cycle Manager does not yet have this source type, add:

- Source type: `ProjectObservatory`
- Discovery method: `cycle_manager_targets()`
- Optional status method: `controlled_statuses()`
- Optional admin refresh method: `collect_controlled_statuses()`
- Target kinds: `SelfReport`, `ControllerStatus`, `InventoryOnly`

Cycle Manager flow:

1. Add the Musical Chairs observatory as a project source.
2. Call `cycle_manager_targets()`.
3. For `SelfReport` targets, call `cycles_status()` and `cycle_manager_metrics()` directly on each target canister.
4. For `ControllerStatus` targets, call `controlled_statuses()` on the observatory.
5. Display `pp_ledger`, `siws_provider`, `frontend`, and `pp_assets` from latest controlled snapshots.
6. Display `backend`, `ponzi_math`, `ponzi_math_sol`, and `shenanigans` from direct self-report.

## Metadata Updates

Static metadata lives in two places:

- `observatory/main.mo` for the on-chain source of truth.
- `cycle-manager.targets.json` for operator onboarding and fallback/manual discovery.

Keep IDs, target kinds, thresholds, and tags aligned when adding or renaming targets.

## Metrics Not Exposed

The self-report metrics intentionally avoid:

- profile contents and display names
- per-player game, balance, Backer, referral, or targeting data
- chat bodies and private user messages
- SOL deposit addresses, signatures, nonce values, transaction bytes, treasury addresses, and signing config
- hidden anti-abuse logic
- raw missed-mint error text

Cycle Manager only needs aggregate operational counters, cycle status, and latest controller-status snapshots.

## Validation

Non-live validation:

```bash
node scripts/validate-cycle-manager-observability.mjs
```

Optional live validation after deployment and controller setup:

```bash
RUN_LIVE_CHECKS=1 DFX_NETWORK=ic node scripts/validate-cycle-manager-observability.mjs
```
