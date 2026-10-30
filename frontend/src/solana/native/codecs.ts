// Hand-written Borsh codecs for the Musical Chairs native Solana program.
//
// DECISION (plan M11, IMPL-46b): NO new dependency. `@coral-xyz/anchor` and
// `@solana/buffer-layout`-style generators are deliberately NOT added to
// package.json. The program's IDL (idl/musical_chairs.json, a byte-identical
// copy of solana/programs/musical_chairs/idl/musical_chairs.json) is the single
// schema: this file interprets its instruction args, account types and defined
// types directly, and the byte vectors the Rust side committed in
// solana/fixtures/ix-encodings.json pin the result (codecs.test.ts). Only
// `bs58` (already a dependency, web3-free) is used, for Pubkey <-> base58.
//
// Layout facts (Anchor 0.32 / Borsh):
//   instruction data = sha256("global:<snake_name>")[..8] ++ Borsh(args)
//   account data     = sha256("account:<TypeName>")[..8] ++ Borsh(fields)
//   ints are little-endian, bool is one byte, Option<T> is 0 | 1 ++ T,
//   [T; N] is N raw elements, enums are a u8 variant index ++ variant fields,
//   Pubkey is 32 raw bytes, structs are their fields in declaration order.
//
// This module is web3-free (no @solana/web3.js import) so it can sit in the
// main bundle. Account data may be LONGER than its Borsh body (Anchor zero-pads
// to the worst-case `SPACE`), so decoding never requires full consumption.

import bs58 from 'bs58';
import idlJson from './idl/musical_chairs.json';
import type {
  CandidatePageData,
  ParticipantPageData,
  RegistryPageData,
  SnapshotPageData,
} from './types';

// ---------------------------------------------------------------------------
// IDL typing
// ---------------------------------------------------------------------------

export type IdlType =
  | string
  | { option: IdlType }
  | { vec: IdlType }
  | { array: [IdlType, number] }
  | { defined: { name: string } };

export interface IdlField {
  name: string;
  type: IdlType;
}

export interface IdlAccountRef {
  name: string;
  writable?: boolean;
  signer?: boolean;
  optional?: boolean;
  address?: string;
  pda?: { seeds: Array<Record<string, unknown>> };
}

export interface IdlInstruction {
  name: string;
  discriminator: number[];
  accounts: IdlAccountRef[];
  args: IdlField[];
}

interface IdlVariant {
  name: string;
  fields?: Array<IdlField | IdlType>;
}

interface IdlTypeDef {
  name: string;
  type: { kind: 'struct'; fields: IdlField[] } | { kind: 'enum'; variants: IdlVariant[] };
}

export interface NativeIdl {
  address: string;
  instructions: IdlInstruction[];
  accounts: Array<{ name: string; discriminator: number[] }>;
  events: Array<{ name: string; discriminator: number[] }>;
  errors: Array<{ code: number; name: string; msg?: string }>;
  types: IdlTypeDef[];
}

export const idl = idlJson as unknown as NativeIdl;

const instructionByName = new Map(idl.instructions.map((i) => [i.name, i]));
const accountDiscriminators = new Map(idl.accounts.map((a) => [a.name, Uint8Array.from(a.discriminator)]));
const typeDefs = new Map(idl.types.map((t) => [t.name, t]));

export function getInstruction(name: string): IdlInstruction {
  const ix = instructionByName.get(name);
  if (!ix) throw new Error(`Unknown instruction "${name}" (not in the program IDL)`);
  return ix;
}

// ---------------------------------------------------------------------------
// Shared-page accounts (Anchor `#[account]` types the IDL does not list)
// ---------------------------------------------------------------------------

/** Page capacity (state.rs PAGE_CAPACITY): rows per registry/snapshot/participant/candidate page. */
export const PAGE_CAPACITY = 8;

/** sha256("account:<Name>")[..8] for the four page accounts (checked against node:crypto in codecs.test.ts). */
export const PAGE_DISCRIMINATORS: Record<string, Uint8Array> = {
  RegistryPage: Uint8Array.from([190, 151, 207, 163, 226, 253, 16, 250]),
  SnapshotPage: Uint8Array.from([249, 175, 151, 41, 53, 179, 39, 246]),
  ParticipantPage: Uint8Array.from([203, 153, 106, 73, 228, 51, 68, 241]),
  CandidatePage: Uint8Array.from([252, 221, 62, 246, 104, 239, 7, 87]),
};

const RENT: IdlType = { defined: { name: 'RentComponents' } };
const PAGE_KEYS: IdlType = { array: ['pubkey', PAGE_CAPACITY] };

// Layouts from solana/programs/musical_chairs/src/state.rs (field order is the wire order).
const EXTRA_TYPES: IdlTypeDef[] = [
  {
    name: 'SnapshotRow',
    type: {
      kind: 'struct',
      fields: [
        { name: 'key', type: 'pubkey' },
        { name: 'series', type: 'u8' },
        { name: 'remaining', type: 'u128' },
        { name: 'slot', type: 'u64' },
      ],
    },
  },
  {
    name: 'RegistryPage',
    type: {
      kind: 'struct',
      fields: [
        { name: 'bump', type: 'u8' },
        { name: 'index', type: 'u64' },
        { name: 'keys', type: PAGE_KEYS },
        { name: 'rent', type: RENT },
      ],
    },
  },
  {
    name: 'SnapshotPage',
    type: {
      kind: 'struct',
      fields: [
        { name: 'bump', type: 'u8' },
        { name: 'index', type: 'u64' },
        { name: 'ticket_id', type: 'u64' },
        { name: 'rows', type: { array: [{ defined: { name: 'SnapshotRow' } }, PAGE_CAPACITY] } },
        { name: 'rent', type: RENT },
      ],
    },
  },
  {
    name: 'ParticipantPage',
    type: {
      kind: 'struct',
      fields: [
        { name: 'bump', type: 'u8' },
        { name: 'round_id', type: 'u64' },
        { name: 'page', type: 'u64' },
        { name: 'len', type: 'u8' },
        { name: 'rows', type: PAGE_KEYS },
        { name: 'rent', type: RENT },
      ],
    },
  },
  {
    name: 'CandidatePage',
    type: {
      kind: 'struct',
      fields: [
        { name: 'bump', type: 'u8' },
        { name: 'close_id', type: 'u64' },
        { name: 'set_tag', type: 'u8' },
        { name: 'page', type: 'u64' },
        { name: 'len', type: 'u8' },
        { name: 'rows', type: PAGE_KEYS },
        { name: 'rent', type: RENT },
      ],
    },
  },
];
for (const def of EXTRA_TYPES) typeDefs.set(def.name, def);

function discriminatorOfAccount(name: string): Uint8Array {
  const disc = accountDiscriminators.get(name) ?? PAGE_DISCRIMINATORS[name];
  if (!disc) throw new Error(`Unknown account type "${name}"`);
  return disc;
}

// ---------------------------------------------------------------------------
// Small byte helpers
// ---------------------------------------------------------------------------

export function bytesToHex(bytes: Uint8Array): string {
  let out = '';
  for (const b of bytes) out += b.toString(16).padStart(2, '0');
  return out;
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) throw new Error('hexToBytes: invalid hex');
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i += 1) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

export function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

const camelCache = new Map<string, string>();
/** `request_rent_funded_s` -> `requestRentFundedS`. */
export function toCamel(snake: string): string {
  let cached = camelCache.get(snake);
  if (cached === undefined) {
    cached = snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
    camelCache.set(snake, cached);
  }
  return cached;
}

/** Anything the encoder accepts as a Pubkey. */
export type PubkeyLike = string | Uint8Array | { toBytes(): Uint8Array } | { toBase58(): string };

export function pubkeyToBytes(value: PubkeyLike): Uint8Array {
  let bytes: Uint8Array;
  if (typeof value === 'string') {
    try {
      bytes = bs58.decode(value);
    } catch {
      throw new Error(`Invalid base58 public key "${value}"`);
    }
  } else if (value instanceof Uint8Array) {
    bytes = value;
  } else if ('toBytes' in value) {
    bytes = value.toBytes();
  } else {
    bytes = bs58.decode(value.toBase58());
  }
  if (bytes.length !== 32) throw new Error(`Public key must be 32 bytes, got ${bytes.length}`);
  return bytes;
}

export function bytesToAddress(bytes: Uint8Array): string {
  return bs58.encode(bytes);
}

// ---------------------------------------------------------------------------
// Borsh writer / reader
// ---------------------------------------------------------------------------

const U64_MAX = (1n << 64n) - 1n;
const U128_MAX = (1n << 128n) - 1n;

function toBigInt(value: unknown, label: string): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') {
    if (!Number.isInteger(value)) throw new Error(`${label}: expected an integer, got ${value}`);
    if (!Number.isSafeInteger(value)) throw new Error(`${label}: ${value} is not a safe integer; pass a bigint`);
    return BigInt(value);
  }
  throw new Error(`${label}: expected a number or bigint, got ${typeof value}`);
}

function checkRange(v: bigint, min: bigint, max: bigint, label: string): bigint {
  if (v < min || v > max) throw new Error(`${label}: ${v} is out of range [${min}, ${max}]`);
  return v;
}

export class BorshWriter {
  private chunks: Uint8Array[] = [];

  private push(bytes: Uint8Array): this {
    this.chunks.push(bytes);
    return this;
  }

  bool(value: boolean): this {
    return this.push(Uint8Array.of(value ? 1 : 0));
  }

  u8(value: number | bigint, label = 'u8'): this {
    return this.push(Uint8Array.of(Number(checkRange(toBigInt(value, label), 0n, 255n, label))));
  }

  u16(value: number | bigint, label = 'u16'): this {
    const v = checkRange(toBigInt(value, label), 0n, 0xffffn, label);
    const out = new Uint8Array(2);
    new DataView(out.buffer).setUint16(0, Number(v), true);
    return this.push(out);
  }

  u32(value: number | bigint, label = 'u32'): this {
    const v = checkRange(toBigInt(value, label), 0n, 0xffffffffn, label);
    const out = new Uint8Array(4);
    new DataView(out.buffer).setUint32(0, Number(v), true);
    return this.push(out);
  }

  u64(value: number | bigint, label = 'u64'): this {
    const v = checkRange(toBigInt(value, label), 0n, U64_MAX, label);
    const out = new Uint8Array(8);
    new DataView(out.buffer).setBigUint64(0, v, true);
    return this.push(out);
  }

  i64(value: number | bigint, label = 'i64'): this {
    const v = checkRange(toBigInt(value, label), -(1n << 63n), (1n << 63n) - 1n, label);
    const out = new Uint8Array(8);
    new DataView(out.buffer).setBigInt64(0, v, true);
    return this.push(out);
  }

  u128(value: number | bigint, label = 'u128'): this {
    const v = checkRange(toBigInt(value, label), 0n, U128_MAX, label);
    const out = new Uint8Array(16);
    const view = new DataView(out.buffer);
    view.setBigUint64(0, v & U64_MAX, true);
    view.setBigUint64(8, v >> 64n, true);
    return this.push(out);
  }

  bytes(value: Uint8Array): this {
    return this.push(Uint8Array.from(value));
  }

  pubkey(value: PubkeyLike): this {
    return this.push(Uint8Array.from(pubkeyToBytes(value)));
  }

  toBytes(): Uint8Array {
    return concatBytes(...this.chunks);
  }
}

export class BorshReader {
  private offset = 0;
  private view: DataView;

  constructor(private readonly data: Uint8Array) {
    this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  }

  get remaining(): number {
    return this.data.length - this.offset;
  }

  private need(n: number): number {
    if (this.offset + n > this.data.length) {
      throw new Error(`Borsh read past the end of data (need ${n} bytes at ${this.offset}, have ${this.data.length}): truncated account`);
    }
    const at = this.offset;
    this.offset += n;
    return at;
  }

  bool(): boolean {
    const byte = this.view.getUint8(this.need(1));
    if (byte > 1) throw new Error(`Invalid bool byte ${byte}`);
    return byte === 1;
  }

  u8(): number {
    return this.view.getUint8(this.need(1));
  }

  u16(): number {
    return this.view.getUint16(this.need(2), true);
  }

  u32(): number {
    return this.view.getUint32(this.need(4), true);
  }

  u64(): bigint {
    return this.view.getBigUint64(this.need(8), true);
  }

  i64(): bigint {
    return this.view.getBigInt64(this.need(8), true);
  }

  u128(): bigint {
    const at = this.need(16);
    return this.view.getBigUint64(at, true) | (this.view.getBigUint64(at + 8, true) << 64n);
  }

  bytes(n: number): Uint8Array {
    const at = this.need(n);
    return Uint8Array.from(this.data.subarray(at, at + n));
  }

  pubkey(): string {
    return bytesToAddress(this.bytes(32));
  }
}

// ---------------------------------------------------------------------------
// IDL-driven value encoding / decoding
// ---------------------------------------------------------------------------

type AnyRecord = Record<string, unknown>;

/** Read `field` from `obj` by its IDL (snake_case) name or its camelCase twin. */
function pick(obj: AnyRecord, name: string): unknown {
  if (Object.prototype.hasOwnProperty.call(obj, name)) return obj[name];
  const camel = toCamel(name);
  if (Object.prototype.hasOwnProperty.call(obj, camel)) return obj[camel];
  return undefined;
}

function asRecord(value: unknown, label: string): AnyRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value) || value instanceof Uint8Array) {
    throw new Error(`${label}: expected an object`);
  }
  return value as AnyRecord;
}

function variantFields(variant: IdlVariant): IdlField[] {
  return (variant.fields ?? []).map((f, i) =>
    typeof f === 'object' && f !== null && 'name' in f && 'type' in f ? (f as IdlField) : { name: String(i), type: f as IdlType },
  );
}

function encodeValue(w: BorshWriter, type: IdlType, value: unknown, label: string): void {
  if (typeof type === 'string') {
    switch (type) {
      case 'bool':
        if (typeof value !== 'boolean') throw new Error(`${label}: expected a boolean`);
        w.bool(value);
        return;
      case 'u8':
        w.u8(toBigInt(value, label), label);
        return;
      case 'u16':
        w.u16(toBigInt(value, label), label);
        return;
      case 'u32':
        w.u32(toBigInt(value, label), label);
        return;
      case 'u64':
        w.u64(toBigInt(value, label), label);
        return;
      case 'i64':
        w.i64(toBigInt(value, label), label);
        return;
      case 'u128':
        w.u128(toBigInt(value, label), label);
        return;
      case 'pubkey':
        if (value === undefined || value === null) throw new Error(`${label}: expected a public key`);
        w.pubkey(value as PubkeyLike);
        return;
      default:
        throw new Error(`${label}: unsupported IDL primitive "${type}"`);
    }
  }
  if ('option' in type) {
    if (value === null || value === undefined) {
      w.bool(false);
    } else {
      w.bool(true);
      encodeValue(w, type.option, value, label);
    }
    return;
  }
  if ('array' in type) {
    const [element, length] = type.array;
    const items = value instanceof Uint8Array ? Array.from(value) : (value as unknown[]);
    if (!Array.isArray(items) || items.length !== length) {
      throw new Error(`${label}: expected an array of ${length} elements`);
    }
    items.forEach((item, i) => encodeValue(w, element, item, `${label}[${i}]`));
    return;
  }
  if ('vec' in type) {
    const items = value instanceof Uint8Array ? Array.from(value) : (value as unknown[]);
    if (!Array.isArray(items)) throw new Error(`${label}: expected an array`);
    w.u32(items.length, `${label}.len`);
    items.forEach((item, i) => encodeValue(w, type.vec, item, `${label}[${i}]`));
    return;
  }
  const def = typeDefs.get(type.defined.name);
  if (!def) throw new Error(`${label}: unknown defined type ${type.defined.name}`);
  if (def.type.kind === 'struct') {
    const obj = asRecord(value, label);
    for (const field of def.type.fields) {
      const fieldValue = pick(obj, field.name);
      if (fieldValue === undefined && !(typeof field.type === 'object' && 'option' in field.type)) {
        throw new Error(`${label}.${field.name}: missing value`);
      }
      encodeValue(w, field.type, fieldValue, `${label}.${field.name}`);
    }
    return;
  }
  // enum
  const name = typeof value === 'string' ? value : (asRecord(value, label).kind as string | undefined);
  const index = def.type.variants.findIndex((v) => v.name === name);
  if (index < 0) {
    throw new Error(`${label}: unknown variant "${String(name)}" of ${def.name}`);
  }
  const variant = def.type.variants[index];
  w.u8(index);
  const fields = variantFields(variant);
  if (fields.length > 0) {
    const obj = asRecord(value, label);
    for (const field of fields) {
      const fieldValue = pick(obj, field.name);
      if (fieldValue === undefined) throw new Error(`${label}.${field.name}: missing value`);
      encodeValue(w, field.type, fieldValue, `${label}.${field.name}`);
    }
  }
}

function decodeValue(r: BorshReader, type: IdlType): unknown {
  if (typeof type === 'string') {
    switch (type) {
      case 'bool':
        return r.bool();
      case 'u8':
        return r.u8();
      case 'u16':
        return r.u16();
      case 'u32':
        return r.u32();
      case 'u64':
        return r.u64();
      case 'i64':
        return r.i64();
      case 'u128':
        return r.u128();
      case 'pubkey':
        return r.pubkey();
      default:
        throw new Error(`Unsupported IDL primitive "${type}"`);
    }
  }
  if ('option' in type) return r.bool() ? decodeValue(r, type.option) : null;
  if ('array' in type) {
    const [element, length] = type.array;
    if (element === 'u8') return r.bytes(length);
    return Array.from({ length }, () => decodeValue(r, element));
  }
  if ('vec' in type) {
    const length = r.u32();
    if (type.vec === 'u8') return r.bytes(length);
    return Array.from({ length }, () => decodeValue(r, type.vec));
  }
  const def = typeDefs.get(type.defined.name);
  if (!def) throw new Error(`Unknown defined type ${type.defined.name}`);
  if (def.type.kind === 'struct') {
    const out: AnyRecord = {};
    for (const field of def.type.fields) out[toCamel(field.name)] = decodeValue(r, field.type);
    return out;
  }
  const index = r.u8();
  const variant = def.type.variants[index];
  if (!variant) throw new Error(`Invalid variant index ${index} for ${def.name}`);
  const out: AnyRecord = { kind: variant.name };
  for (const field of variantFields(variant)) out[toCamel(field.name)] = decodeValue(r, field.type);
  return out;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Anchor's 8-byte instruction discriminator, straight from the IDL. */
export function instructionDiscriminator(name: string): Uint8Array {
  return Uint8Array.from(getInstruction(name).discriminator);
}

/**
 * `discriminator ++ Borsh(args)` for any of the program's instructions.
 * `args` is keyed by the IDL's snake_case names or their camelCase twins.
 */
export function encodeInstructionData(name: string, args: AnyRecord = {}): Uint8Array {
  const ix = getInstruction(name);
  const w = new BorshWriter();
  w.bytes(Uint8Array.from(ix.discriminator));
  for (const arg of ix.args) {
    const value = pick(args, arg.name);
    if (value === undefined && !(typeof arg.type === 'object' && 'option' in arg.type)) {
      throw new Error(`${name}: missing argument "${arg.name}"`);
    }
    encodeValue(w, arg.type, value, `${name}.${arg.name}`);
  }
  return w.toBytes();
}

/** `discriminator ++ Borsh(fields)` for an IDL account or page type (used by fixtures and tests). */
export function encodeAccountData(name: string, value: AnyRecord): Uint8Array {
  const w = new BorshWriter();
  w.bytes(discriminatorOfAccount(name));
  encodeValue(w, { defined: { name } }, value, name);
  return w.toBytes();
}

/**
 * Decode an account's data: checks the 8-byte discriminator, then reads the
 * Borsh body. Trailing bytes (Anchor's zero padding) are ignored.
 */
export function decodeAccountData<T>(name: string, data: Uint8Array): T {
  const expected = discriminatorOfAccount(name);
  if (data.length < 8) throw new Error(`${name}: account data is ${data.length} bytes (truncated)`);
  for (let i = 0; i < 8; i += 1) {
    if (data[i] !== expected[i]) {
      throw new Error(`${name}: account discriminator mismatch (this is not a ${name} account)`);
    }
  }
  return decodeValue(new BorshReader(data.subarray(8)), { defined: { name } }) as T;
}

export const decodeRegistryPage = (data: Uint8Array) => decodeAccountData<RegistryPageData>('RegistryPage', data);
export const decodeSnapshotPage = (data: Uint8Array) => decodeAccountData<SnapshotPageData>('SnapshotPage', data);
export const decodeParticipantPage = (data: Uint8Array) => decodeAccountData<ParticipantPageData>('ParticipantPage', data);
export const decodeCandidatePage = (data: Uint8Array) => decodeAccountData<CandidatePageData>('CandidatePage', data);

/** Program error code -> IDL error name (6000+ are the program's; others are Anchor framework codes). */
export function programErrorName(code: number): string | undefined {
  return idl.errors.find((e) => e.code === code)?.name;
}
