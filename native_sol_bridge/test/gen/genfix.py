#!/usr/bin/env python3
"""Regenerates native_sol_bridge/test/fixtures.test.mo from solana/fixtures/rpc/*.json.

Run:  python3 -I native_sol_bridge/test/gen/genfix.py     (from anywhere; paths are relative to this file)

Each fixture is emitted twice: the fixture's own JSON-shaped fields, and the typed SOL RPC
canister result whose transaction is the same signed message re-serialized to the wire format
(b58.py / wire.py; every signature was verified over those exact bytes with ed25519 when the
generator was written). Stdlib only (no network).
"""
import json,glob,os,sys,base64
sys.path.insert(0,os.path.dirname(__file__))
from b58 import dec,enc
from wire import wire_tx
# Repo root, relative to this script: native_sol_bridge/test/gen/genfix.py -> ../../..
R=os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','..','..'))
ED=R+'/solana/fixtures/rpc'
manifest=json.load(open(ED+'/manifest.json'))
mevents={t['file']:t['events'] for t in manifest['transactions']}
mdisc={t['file']:t['top_level_program_ix_discriminators'] for t in manifest['transactions']}
def q(s): return '"'+s.replace('\\','\\\\').replace('"','\\"')+'"'
def blob(bs): return '"'+''.join('\\%02x'%b for b in bs)+'"'
def nat_list(l): return '['+', '.join(str(x) for x in l)+']'
def optn(x): return 'null' if x is None else '?%d'%x
def json_ix(x):
    return '{ programIdIndex = %d; accounts = %s; data = %s; stackHeight = %s }'%(x['programIdIndex'],nat_list(x['accounts']),q(x['data']),optn(x.get('stackHeight')))
def rpc_ix(x):
    return '#compiled { data = %s; accounts = %s; programIdIndex = %d; stackHeight = %s }'%(q(x['data']),blob(x['accounts']),x['programIdIndex'],optn(x.get('stackHeight')))
out=[]
out.append('''import Decode "../Decode";
import Rpc "../Rpc";

/// GENERATED test data, do not edit by hand.
/// Source: solana/fixtures/rpc/{flows,closing}-NNN.json (finalized getTransaction
/// results captured from two local-validator runs, legacy transactions,
/// encoding json, maxSupportedTransactionVersion 0) and manifest.json.
///
/// Each fixture appears twice:
///   json - the fixture's own JSON-shaped fields (accountKeys, header, instructions,
///          meta.innerInstructions with base58 data and stackHeight);
///   rpc  - the typed SOL RPC canister result a live call would return: the SAME
///          signed message re-serialized to the wire format and base64-encoded
///          (the generator verified every signature over those exact bytes with
///          ed25519, so these are the real transactions' bytes), plus the typed meta.
module {
    public let PROGRAM : Text = "7aSvuR19bh9WBHoPC5ceLNvP6txxFkowTJ2FSi9fnf3o";
    /// findProgramAddress(["__event_authority"], PROGRAM): bump 254.
    public let EVENT_AUTHORITY : Text = "8c7gNtfPuYRUHtp97qpzLKsSLFgogKYC7ZjVcA8MEo8m";
    /// Cluster domain of the local validator runs (32 bytes).
    public let LOCALNET_DOMAIN_TEXT : Text = "musical-chairs-localnet-dev!!!!!";

    public type Fx = {
        name : Text;
        /// Event names the manifest lists for this transaction, in order.
        manifest_events : [Text];
        json : Decode.JsonTx;
        rpc : Rpc.EncodedConfirmedTransactionWithStatusMeta;
    };

    public let all : [Fx] = [''')
files=sorted(glob.glob(ED+'/*-0*.json'))
for f in files:
    name=os.path.basename(f)
    r=json.load(open(f))['result']
    t=r['transaction']; m=t['message']; meta=r['meta']
    assert meta['err'] is None
    ver=r.get('version')
    assert ver=='legacy'
    inner=meta['innerInstructions']
    la=meta.get('loadedAddresses') or {'writable':[],'readonly':[]}
    w=base64.b64encode(wire_tx(t)).decode()
    out.append('        {')
    out.append('            name = %s;'%q(name[:-5]))
    out.append('            manifest_events = [%s];'%', '.join(q(e) for e in mevents[name]))
    out.append('            json = {')
    out.append('                slot = %d; version = ?#legacy; failed = false;'%r['slot'])
    out.append('                signatures = [%s];'%', '.join(q(s) for s in t['signatures']))
    out.append('                numRequiredSignatures = %d;'%m['header']['numRequiredSignatures'])
    out.append('                accountKeys = [%s];'%', '.join(q(k) for k in m['accountKeys']))
    out.append('                loadedAddresses = ?{ writable = []; readonly = [] };')
    out.append('                instructions = [%s];'%', '.join(json_ix(x) for x in m['instructions']))
    out.append('                innerInstructions = ?[%s];'%', '.join('{ index = %d; instructions = [%s] }'%(g['index'],', '.join(json_ix(x) for x in g['instructions'])) for g in inner))
    out.append('            };')
    out.append('            rpc = {')
    out.append('                slot = %d; blockTime = null;'%r['slot'])
    out.append('                transaction = {')
    out.append('                    meta = ?{')
    out.append('                        fee = %d; status = #Ok;'%meta['fee'])
    out.append('                        innerInstructions = ?[%s];'%', '.join('{ index = %d; instructions = [%s] }'%(g['index'],', '.join(rpc_ix(x) for x in g['instructions'])) for g in inner))
    out.append('                        postTokenBalances = null; preBalances = []; postBalances = []; returnData = null; logMessages = null;')
    out.append('                        rewards = null; loadedAddresses = ?{ writable = []; readonly = [] }; preTokenBalances = null;')
    out.append('                        computeUnitsConsumed = null; cost_units = null;')
    out.append('                    };')
    out.append('                    transaction = #binary(%s, #base64);'%q(w))
    out.append('                    version = ?#legacy;')
    out.append('                };')
    out.append('            };')
    out.append('        },')
out.append('''    ];

    public func get(name : Text) : Fx {
        for (f in all.vals()) { if (f.name == name) { return f } };
        assert false;
        all[0];
    };
};''')
open(R+'/native_sol_bridge/test/fixtures.test.mo','w').write('\n'.join(out)+'\n')
print(len(files),'fixtures;',os.path.getsize(R+'/native_sol_bridge/test/fixtures.test.mo'),'bytes')
