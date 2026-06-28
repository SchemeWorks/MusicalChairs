#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = process.cwd();
const NETWORK = process.env.DFX_NETWORK || "ic";
const RUN_LIVE_CHECKS = process.env.RUN_LIVE_CHECKS === "1";
const OBSERVATORY_CONTROLLER_PLACEHOLDER = "<musical_chairs_observatory.ic>";

const OBSERVATORY_CANISTER = "musical_chairs_observatory";

const EXPECTED_TARGETS = [
  ["backend", "5zxxg-tyaaa-aaaac-qeckq-cai", "SelfReport"],
  ["ponzi_math", "guy42-yqaaa-aaaaj-qr5pq-cai", "SelfReport"],
  ["ponzi_math_sol", "spc6q-xyaaa-aaaac-qg2ma-cai", "SelfReport"],
  ["shenanigans", "j56tm-oaaaa-aaaac-qf34q-cai", "SelfReport"],
  ["pp_ledger", "5xv2o-iiaaa-aaaac-qeclq-cai", "ControllerStatus"],
  ["siws_provider", "tcm26-yqaaa-aaaac-qg2lq-cai", "ControllerStatus"],
  ["frontend", "5qu42-fqaaa-aaaac-qecla-cai", "ControllerStatus"],
  ["pp_assets", "4236a-haaaa-aaaac-qecma-cai", "ControllerStatus"],
];

const errors = [];
const warnings = [];

function rel(file) {
  return path.join(ROOT, file);
}

function readJson(file) {
  try {
    return JSON.parse(readFileSync(rel(file), "utf8"));
  } catch (error) {
    errors.push(`${file}: ${error.message}`);
    return undefined;
  }
}

function expect(condition, message) {
  if (!condition) errors.push(message);
}

function warn(condition, message) {
  if (!condition) warnings.push(message);
}

function canisterId(canisterIds, name) {
  return canisterIds?.[name]?.[NETWORK] ?? canisterIds?.[name]?.ic;
}

function candidVariantName(kind) {
  if (typeof kind === "string") return kind;
  if (kind && typeof kind === "object") {
    const keys = Object.keys(kind);
    if (keys.length === 1) return keys[0];
  }
  return undefined;
}

function callDfx(canisterNameOrId, method) {
  return execFileSync(
    "dfx",
    ["canister", "--network", NETWORK, "call", canisterNameOrId, method, "()"],
    { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
}

const dfx = readJson("dfx.json");
const canisterIds = readJson("canister_ids.json");

expect(
  dfx?.canisters?.[OBSERVATORY_CANISTER]?.type === "motoko",
  `dfx.json must define ${OBSERVATORY_CANISTER} as a Motoko canister`,
);
expect(
  dfx?.canisters?.[OBSERVATORY_CANISTER]?.main === "observatory/main.mo",
  `dfx.json ${OBSERVATORY_CANISTER}.main must be observatory/main.mo`,
);

for (const [name, expectedId] of EXPECTED_TARGETS) {
  expect(
    canisterId(canisterIds, name) === expectedId,
    `canister_ids.json ${name}.${NETWORK} must be ${expectedId}`,
  );
}

const observatoryId = canisterId(canisterIds, OBSERVATORY_CANISTER);
warn(
  Boolean(observatoryId),
  `canister_ids.json has no ${OBSERVATORY_CANISTER}.${NETWORK}; deploy/create the observatory before live onboarding`,
);

const manifestPath = "cycle-manager.targets.json";
expect(existsSync(rel(manifestPath)), `${manifestPath} must exist`);

if (existsSync(rel(manifestPath))) {
  const manifest = readJson(manifestPath);
  expect(manifest?.project === "musical-chairs", "manifest project must be musical-chairs");
  expect(manifest?.schema_version === 2, "manifest schema_version must be 2");
  expect(
    manifest?.discovery?.type === "project_observatory",
    "manifest discovery.type must be project_observatory",
  );
  expect(
    manifest?.discovery?.canister_name === OBSERVATORY_CANISTER,
    `manifest discovery.canister_name must be ${OBSERVATORY_CANISTER}`,
  );
  if (observatoryId) {
    expect(
      manifest?.discovery?.canister_id === observatoryId,
      `manifest discovery.canister_id must match canister_ids.json ${OBSERVATORY_CANISTER}.${NETWORK}`,
    );
  } else {
    expect(
      manifest?.discovery?.canister_id === OBSERVATORY_CONTROLLER_PLACEHOLDER,
      `manifest discovery.canister_id must be ${OBSERVATORY_CONTROLLER_PLACEHOLDER} until deploy fills canister_ids.json`,
    );
  }
  expect(
    manifest?.discovery?.methods?.targets === "cycle_manager_targets",
    "manifest discovery.methods.targets must be cycle_manager_targets",
  );
  expect(
    manifest?.discovery?.methods?.collect_controlled_statuses === "collect_controlled_statuses",
    "manifest discovery.methods.collect_controlled_statuses must be collect_controlled_statuses",
  );
  expect(
    manifest?.discovery?.methods?.controlled_statuses === "controlled_statuses",
    "manifest discovery.methods.controlled_statuses must be controlled_statuses",
  );
  expect(Array.isArray(manifest?.targets), "manifest targets must be an array");

  const targetsByName = new Map((manifest?.targets ?? []).map((target) => [target.canister_name, target]));
  for (const [name, expectedId, expectedKind] of EXPECTED_TARGETS) {
    const target = targetsByName.get(name);
    expect(Boolean(target), `manifest missing target ${name}`);
    if (!target) continue;
    expect(target.canister_id === expectedId, `manifest ${name}.canister_id must be ${expectedId}`);
    expect(
      candidVariantName(target.kind) === expectedKind,
      `manifest ${name}.kind must be ${expectedKind}`,
    );
    expect(target.project === "musical-chairs", `manifest ${name}.project must be musical-chairs`);
    expect(
      Array.isArray(target.expected_controllers),
      `manifest ${name}.expected_controllers must be an array`,
    );
    if (expectedKind === "ControllerStatus") {
      const expectedController = observatoryId || OBSERVATORY_CONTROLLER_PLACEHOLDER;
      expect(
        target.expected_controllers.includes(expectedController),
        `manifest ${name}.expected_controllers must include ${expectedController}`,
      );
    }
    expect(
      Array.isArray(target.tags) && target.tags.length > 0,
      `manifest ${name}.tags must be a non-empty array`,
    );
  }
}

expect(existsSync(rel("docs/cycle-manager-observability.md")), "docs/cycle-manager-observability.md must exist");
expect(existsSync(rel("observatory/main.mo")), "observatory/main.mo must exist");

if (existsSync(rel("observatory/types.mo"))) {
  const typesMo = readFileSync(rel("observatory/types.mo"), "utf8");
  expect(
    typesMo.includes("controllers : [Principal]"),
    "observatory ControlledStatus must include observed controllers",
  );
}

if (existsSync(rel("frontend/src/declarations/musical_chairs_observatory/musical_chairs_observatory.did"))) {
  const observatoryDid = readFileSync(
    rel("frontend/src/declarations/musical_chairs_observatory/musical_chairs_observatory.did"),
    "utf8",
  );
  expect(
    observatoryDid.includes("controllers: vec principal"),
    "observatory Candid declaration must include ControlledStatus.controllers",
  );
}

if (RUN_LIVE_CHECKS) {
  if (!observatoryId) {
    errors.push(`RUN_LIVE_CHECKS=1 requires ${OBSERVATORY_CANISTER}.${NETWORK} in canister_ids.json`);
  } else {
    for (const [canister, method] of [
      [OBSERVATORY_CANISTER, "observatory_version"],
      [OBSERVATORY_CANISTER, "cycle_manager_targets"],
      [OBSERVATORY_CANISTER, "collect_controlled_statuses"],
      [OBSERVATORY_CANISTER, "controlled_statuses"],
      ["backend", "cycles_status"],
      ["backend", "cycle_manager_metrics"],
      ["ponzi_math", "cycles_status"],
      ["ponzi_math", "cycle_manager_metrics"],
      ["ponzi_math_sol", "cycles_status"],
      ["ponzi_math_sol", "cycle_manager_metrics"],
      ["shenanigans", "cycles_status"],
      ["shenanigans", "cycle_manager_metrics"],
    ]) {
      try {
        callDfx(canister, method);
      } catch (error) {
        errors.push(`dfx call ${canister}.${method} failed: ${error.stderr || error.message}`);
      }
    }
  }
}

for (const message of warnings) {
  console.warn(`WARN ${message}`);
}

if (errors.length > 0) {
  console.error("Cycle Manager observability validation failed:");
  for (const message of errors) {
    console.error(`- ${message}`);
  }
  process.exit(1);
}

console.log("Cycle Manager observability validation passed");
