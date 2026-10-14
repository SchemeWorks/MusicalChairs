// Test-only helpers: build on-chain account bytes from the IDL and fake RPC /
// wallet objects. Imported by *.test.ts only; no app code imports this file, so
// it never reaches the production bundle.

import { readFileSync } from 'node:fs';
import { Keypair, PublicKey, Transaction } from '@solana/web3.js';
import { expect, vi } from 'vitest';
import { encodeAccountData, idl, toCamel, type IdlType } from './codecs';
import type { NativeConnection } from './client';
import type { NativeWallet } from './wallet';

const ZERO_KEY = '11111111111111111111111111111111';

type Rec = Record<string, unknown>;

function zeroOf(type: IdlType): unknown {
  if (type === 'bool') return false;
  if (type === 'pubkey') return ZERO_KEY;
  if (typeof type === 'string') return 0n;
  if ('option' in type) return null;
  if ('array' in type) {
    const [element, length] = type.array;
    return Array.from({ length }, () => zeroOf(element));
  }
  if ('vec' in type) return [];
  const def = idl.types.find((t) => t.name === type.defined.name);
  if (!def) throw new Error(`no type ${type.defined.name}`);
  if (def.type.kind === 'struct') return Object.fromEntries(def.type.fields.map((f) => [f.name, zeroOf(f.type)]));
  return { kind: def.type.variants[0].name };
}

const EXTRA_ZERO: Record<string, Rec> = {
  RegistryPage: { bump: 0n, index: 0n, keys: Array(8).fill(ZERO_KEY), rent: { ops: 0n, owner: 0n, owner_refund: ZERO_KEY } },
  SnapshotPage: {
    bump: 0n,
    index: 0n,
    ticket_id: 0n,
    rows: Array.from({ length: 8 }, () => ({ key: ZERO_KEY, series: 0n, remaining: 0n, slot: 0n })),
    rent: { ops: 0n, owner: 0n, owner_refund: ZERO_KEY },
  },
  ParticipantPage: { bump: 0n, round_id: 0n, page: 0n, len: 0n, rows: Array(8).fill(ZERO_KEY), rent: { ops: 0n, owner: 0n, owner_refund: ZERO_KEY } },
  CandidatePage: { bump: 0n, close_id: 0n, set_tag: 0n, page: 0n, len: 0n, rows: Array(8).fill(ZERO_KEY), rent: { ops: 0n, owner: 0n, owner_refund: ZERO_KEY } },
};

/** Account bytes for `name` with every field zero except `overrides` (camelCase or snake_case keys). */
export function buildAccount(name: string, overrides: Rec = {}): Uint8Array {
  const base = (EXTRA_ZERO[name] ?? (zeroOf({ defined: { name } }) as Rec)) as Rec;
  const merged: Rec = { ...base };
  for (const [key, value] of Object.entries(overrides)) {
    const snake = Object.keys(base).find((k) => k === key || toCamel(k) === key);
    if (!snake) throw new Error(`${name} has no field "${key}"`);
    merged[snake] = value;
  }
  return encodeAccountData(name, merged);
}

export interface FakeAccount {
  data: Uint8Array;
  owner: PublicKey;
  lamports?: number;
}

/** A fake RPC connection backed by an address -> account map. */
export function fakeConnection(
  accounts: Map<string, FakeAccount>,
  extra: Partial<Record<keyof NativeConnection, unknown>> = {},
): NativeConnection & Record<keyof NativeConnection, ReturnType<typeof vi.fn>> {
  const info = (key: PublicKey | string) => {
    const account = accounts.get(typeof key === 'string' ? key : key.toBase58());
    return account
      ? { data: Buffer.from(account.data), owner: account.owner, lamports: account.lamports ?? 1_000_000, executable: false, rentEpoch: 0 }
      : null;
  };
  const conn = {
    getAccountInfo: vi.fn(async (key: PublicKey | string) => info(key)),
    getMultipleAccountsInfo: vi.fn(async (keys: Array<PublicKey | string>) => keys.map(info)),
    getLatestBlockhash: vi.fn(async () => ({ blockhash: '4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi', lastValidBlockHeight: 1_000 })),
    sendRawTransaction: vi.fn(async (_raw: Uint8Array) => 'sig'),
    getSignatureStatuses: vi.fn(async () => ({ context: { slot: 1 }, value: [null] })),
    getBlockHeight: vi.fn(async () => 10),
    getProgramAccounts: vi.fn(async () => []),
    ...extra,
  };
  return conn as unknown as NativeConnection & Record<keyof NativeConnection, ReturnType<typeof vi.fn>>;
}

/** A wallet that signs with a real throwaway keypair; `signTransaction` is a spy. */
export function fakeWallet(keypair: Keypair = Keypair.generate()): NativeWallet & {
  keypair: Keypair;
  signTransaction: ReturnType<typeof vi.fn>;
} {
  return {
    keypair,
    publicKey: keypair.publicKey.toBase58(),
    signTransaction: vi.fn(async (tx: Transaction) => {
      tx.partialSign(keypair);
      return tx;
    }),
  };
}

// ---------------------------------------------------------------------------
// solana/fixtures/ix-encodings.json (byte vectors from the Rust side)
// ---------------------------------------------------------------------------

export const REPO_ROOT = new URL('../../../../', import.meta.url);

export interface FixtureEntry {
  name: string;
  discriminator_hex: string;
  sample: Record<string, unknown>;
  data_hex: string;
  data_len: number;
}
export const ixFixture = JSON.parse(
  readFileSync(new URL('solana/fixtures/ix-encodings.json', REPO_ROOT), 'utf8'),
) as { program: string; instructions: FixtureEntry[] };

/** `open_position_compound15` -> `open_position`; `provision_page_candidate_caller` -> `provision_page`. */
export function baseName(name: string): string {
  for (const suffix of ['_compound15', '_compound30', '_candidate_caller']) {
    if (name.endsWith(suffix)) return name.slice(0, -suffix.length);
  }
  return name;
}

// --- Rust `Debug` samples ---------------------------------------------------
// The two struct samples (`InitializeArgs`, `ConfigTerms`) are Rust Debug
// strings, not JSON. They are parsed with the IDL's field types, so a pubkey
// like 11111111111111111111111111111111 is never mistaken for a number.

export const typeDef = (name: string) => {
  const def = (idl.types as Array<{ name: string; type: { kind: string; fields: Array<{ name: string; type: unknown }> } }>).find(
    (t) => t.name === name,
  );
  if (!def) throw new Error(`IDL has no type ${name}`);
  return def;
};

function coerce(type: unknown, raw: string | string[]): unknown {
  if (typeof type === 'object' && type !== null && 'array' in type) {
    const [element] = (type as { array: [IdlType, number] }).array;
    return (raw as string[]).map((item) => coerce(element, item));
  }
  const text = raw as string;
  if (type === 'pubkey') return text;
  if (type === 'bool') return text === 'true';
  if (typeof type === 'string' && /^[ui]\d+$/.test(type)) return BigInt(text);
  throw new Error(`unsupported Debug field type ${JSON.stringify(type)}`);
}

function parseDebugStruct(typeName: string, text: string): Record<string, unknown> {
  const body = text.slice(text.indexOf('{') + 1, text.lastIndexOf('}'));
  const parts: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of body) {
    if (ch === '[') depth += 1;
    if (ch === ']') depth -= 1;
    if (ch === ',' && depth === 0) {
      parts.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  if (current.trim()) parts.push(current);
  const raws = new Map<string, string | string[]>();
  for (const part of parts) {
    const colon = part.indexOf(':');
    const key = part.slice(0, colon).trim();
    const value = part.slice(colon + 1).trim();
    raws.set(key, value.startsWith('[') ? value.slice(1, -1).split(',').map((s) => s.trim()) : value);
  }
  const out: Record<string, unknown> = {};
  for (const field of typeDef(typeName).type.fields) {
    const raw = raws.get(field.name);
    if (raw === undefined) throw new Error(`${typeName}.${field.name} missing from the Debug sample`);
    out[field.name] = coerce(field.type, raw);
  }
  expect(raws.size, `${typeName}: every Debug field is an IDL field`).toBe(typeDef(typeName).type.fields.length);
  return out;
}

/** Turn a fixture sample into encoder arguments. */
export function argsFromSample(name: string, sample: Record<string, unknown>): Record<string, unknown> {
  const args: Record<string, unknown> = { ...sample };
  if (name === 'initialize') args.args = parseDebugStruct('InitializeArgs', String(sample.args));
  if (name === 'stage_config_version') args.terms = parseDebugStruct('ConfigTerms', String(sample.terms));
  if (name === 'provision_page') {
    args.kind = parseDebugPageKind(String(sample.kind));
  }
  if (name === 'rotate_recipient' || name === 'close_shared_account') {
    if (typeof sample.kind === 'string') args.kind = parseDebugPageKind(String(sample.kind));
  }
  return args;
}

/** `Participant { round: 3 }` -> `{ kind: 'Participant', round: 3n }`; bare names stay `{ kind }`. */
function parseDebugPageKind(text: string): Record<string, unknown> {
  const match = /^(\w+)(?:\s*\{(.*)\})?$/.exec(text.trim());
  if (!match) throw new Error(`cannot parse PageKind ${text}`);
  const out: Record<string, unknown> = { kind: match[1] };
  if (match[2]) {
    for (const pair of match[2].split(',')) {
      const [k, v] = pair.split(':').map((s) => s.trim());
      out[k] = BigInt(v);
    }
  }
  return out;
}


// ---------------------------------------------------------------------------
// solana/fixtures/*.json readers
// ---------------------------------------------------------------------------

/** Fixture JSON carries some u64 values above 2^53 as bare numbers; quote them so they parse exactly. */
export function readFixtureJson<T>(name: string): T {
  const raw = readFileSync(new URL(`solana/fixtures/${name}`, REPO_ROOT), 'utf8');
  return JSON.parse(raw.replace(/("(?:round_id|candidate_count|ordinal|counter|loss)"\s*:\s*)(\d{16,})/g, '$1"$2"')) as T;
}

export interface SamplingFixture {
  output_vectors: Array<{
    label: string;
    output_hex: string;
    round_id: string | number;
    candidate_count: string | number;
    ordinal: string | number | null;
    counter: number | null;
  }>;
  candidates: { set_tag: number; rows: Array<{ owner_hex: string; loss: string }>; empty_digest_hex: string; digest_hex: string };
  protocol_seed: {
    program_id: string;
    round_id: number;
    close_id: number;
    set_tag: number;
    selected_count: number;
    sealed_digest_hex: string;
    vectors: Array<{ cluster: string; cluster_domain: string; seed_hex: string }>;
  };
}

export const readSamplingFixture = (): SamplingFixture => readFixtureJson<SamplingFixture>('sampling.json');
