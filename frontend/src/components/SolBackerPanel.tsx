import { useState, useEffect, useMemo, useRef } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy, AlertTriangle, TrendingUp, Loader2, Gem } from 'lucide-react';
import {
  usePrepareBackerDeposit,
  useGetMyPendingBackerIntents,
  usePokeMyDeposit,
  useGetMintConfig,
} from '../hooks/useQueries';
import { usePonziMathSolActor } from '../hooks/usePonziMathSolActor';
import { useWallet } from '../hooks/useWallet';
import { triggerConfetti } from './ConfettiCanvas';
import { formatSOL, formatSolFloat, parseSOL, LAMPORTS_PER_SOL } from '../solana/lamports';
import { sendSolDeposit } from '../solana/sendSolDeposit';
import { UPSTREAM_BACKER_BONUS } from '../lib/gameConstants';

// Series A backing minimum — matches ponzi_math_sol MIN_BACKER_LAMPORTS (0.05 SOL).
const MIN_BACKER_SOL = 0.05;
const MIN_LAMPORTS = parseSOL(String(MIN_BACKER_SOL));

type Flow =
  | { kind: 'input' }
  | { kind: 'awaitingWallet'; lamports: bigint; depositAddress?: string; intentId?: bigint }
  | { kind: 'opening'; lamports: bigint; intentId: bigint }
  | { kind: 'manual'; depositAddress: string; lamports: bigint; intentId: bigint; note?: string }
  | { kind: 'opened'; lamports: bigint };

function friendlyWalletError(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/reject|denied|cancel/i.test(msg)) return 'Wallet request was cancelled.';
  return msg || 'Could not complete the transfer from your wallet.';
}

export default function SolBackerPanel() {
  const { actor } = usePonziMathSolActor();
  const { solanaPubkey } = useWallet();
  const prepareMut = usePrepareBackerDeposit();
  const pokeMut = usePokeMyDeposit();
  const { data: pendingBackerIntents } = useGetMyPendingBackerIntents();
  const { data: mintConfig } = useGetMintConfig();

  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [flow, setFlow] = useState<Flow>({ kind: 'input' });
  // Monotonic id for the active one-click op. Bumped on each start AND on cancel,
  // so a slow/hung sendSolDeposit that finally settles after the user bailed can't
  // stomp the UI (its captured gen no longer matches opGenRef.current).
  const opGenRef = useRef(0);

  const ppPerSol = mintConfig ? Number(mintConfig.backerPpPerSol) : 0;

  const lamports = useMemo(() => {
    try { return solInput.trim() ? parseSOL(solInput) : 0n; } catch { return 0n; }
  }, [solInput]);

  const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
  const belowMin = lamports > 0n && lamports < MIN_LAMPORTS;
  const canDeposit = !!actor && !!solanaPubkey && lamports >= MIN_LAMPORTS && !prepareMut.isPending;

  // No Front-End Load on backer deposits — full gross to pot, entitlement = gross * 1.24.
  const expectedReturn = solFloat * (1 + UPSTREAM_BACKER_BONUS);
  const projectedPP = solFloat > 0 ? Math.round(solFloat * ppPerSol) : 0;

  // Credit detection: only declare "opened" once we've SEEN our intent in the
  // pending set and then watched it disappear (fulfilled by detection). Guarding
  // on "seen first" avoids a false positive from the refetch lag right after
  // prepare, before the pending list includes the new intent.
  const sawIntent = useRef(false);
  useEffect(() => {
    if (flow.kind !== 'opening' && flow.kind !== 'manual') { sawIntent.current = false; return; }
    if (!pendingBackerIntents) return;
    const present = pendingBackerIntents.some((i) => i.id === flow.intentId);
    if (present) { sawIntent.current = true; return; }
    if (sawIntent.current) {
      triggerConfetti();
      setFlow({ kind: 'opened', lamports: flow.lamports });
    }
  }, [flow, pendingBackerIntents]);

  // While opening or in the manual fallback, poke the canister to scan for the
  // deposit — immediately, then every 6s for ~36s — so the position registers
  // within seconds rather than waiting for the 60s timer (which still backstops).
  useEffect(() => {
    if (flow.kind !== 'opening' && flow.kind !== 'manual') return;
    const poke = () => { pokeMut.mutateAsync().catch(() => {}); };
    poke();
    let tries = 0;
    const id = setInterval(() => {
      tries += 1;
      poke();
      if (tries >= 6) clearInterval(id);
    }, 6000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flow.kind]);

  const handleCopy = async (text: string) => {
    let ok = false;
    try {
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); ok = true; }
    } catch { ok = false; }
    if (!ok) {
      try {
        const ta = document.createElement('textarea');
        ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
        document.body.appendChild(ta); ta.focus(); ta.select();
        ok = document.execCommand('copy');
        document.body.removeChild(ta);
      } catch { ok = false; }
    }
    if (ok) { setCopied(true); setTimeout(() => setCopied(false), 1500); }
  };

  const handleOneClick = async () => {
    if (!canDeposit || !solanaPubkey) return;
    const gen = ++opGenRef.current;
    setFlow({ kind: 'awaitingWallet', lamports });
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ expectedAmountLamports: lamports });
    } catch {
      if (opGenRef.current === gen) setFlow({ kind: 'input' }); // prepareMut.isError surfaces the message in the input view
      return;
    }
    if (opGenRef.current !== gen) return; // cancelled while preparing
    // Stash the prepared address + intentId in the spinner state so Cancel can drop
    // to the manual view (and the credit detector still has the intent) if the wallet hangs.
    setFlow({ kind: 'awaitingWallet', lamports, depositAddress: prepared.depositAddress, intentId: prepared.intentId });
    try {
      await sendSolDeposit({ toAddress: prepared.depositAddress, lamports, expectedPubkey: solanaPubkey });
      if (opGenRef.current !== gen) return; // user bailed; ignore the late success
      setFlow({ kind: 'opening', lamports, intentId: prepared.intentId });
    } catch (e) {
      if (opGenRef.current !== gen) return; // user bailed; ignore the late failure
      setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, intentId: prepared.intentId, note: `${friendlyWalletError(e)} Send manually below — your Series A position still registers.` });
    }
  };

  // Escape hatch from the "Confirm in your wallet…" spinner (e.g. Phantom popup
  // hung or dismissed without resolving). We can't truly abort the wallet promise,
  // so we bump opGenRef (its late settle is ignored) and drop to the manual view
  // when an address was prepared — the detector keeps polling, so a transfer the
  // user DID approve still registers. Copy avoids implying a (double-)send is needed.
  const handleCancelWallet = () => {
    opGenRef.current += 1;
    if (flow.kind === 'awaitingWallet' && flow.depositAddress && flow.intentId !== undefined) {
      setFlow({
        kind: 'manual',
        depositAddress: flow.depositAddress,
        lamports: flow.lamports,
        intentId: flow.intentId,
        note: "Stopped waiting on your wallet. If you already approved the transfer in Phantom, your Series A position registers automatically — don't send again. Otherwise, send manually below or start over.",
      });
    } else {
      setFlow({ kind: 'input' });
    }
  };

  const handleManual = async () => {
    if (!canDeposit) return;
    let prepared: { intentId: bigint; depositAddress: string };
    try {
      prepared = await prepareMut.mutateAsync({ expectedAmountLamports: lamports });
    } catch { return; }
    setFlow({ kind: 'manual', depositAddress: prepared.depositAddress, lamports, intentId: prepared.intentId });
  };

  const handleCheckNow = () => { pokeMut.mutateAsync().catch(() => {}); };
  const handleStartOver = () => { setFlow({ kind: 'input' }); setSolInput(''); prepareMut.reset(); pokeMut.reset(); };

  if (!actor) {
    return (
      <div className="mc-card p-6 text-center">
        <p className="text-sm mc-text-dim">Connecting your Solana session…</p>
      </div>
    );
  }

  if (flow.kind === 'opened') {
    const sol = Number(flow.lamports) / Number(LAMPORTS_PER_SOL);
    return (
      <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
        <Gem className="h-8 w-8 mc-text-cyan mx-auto" />
        <div className="font-display text-xl mc-text-primary">You're a Series A backer.</div>
        <p className="text-sm mc-text-dim">
          {formatSolFloat(sol)} SOL in — entitlement {formatSolFloat(sol * (1 + UPSTREAM_BACKER_BONUS))} SOL (24% bonus). Your Ponzi Points mint shortly.
        </p>
        <button onClick={handleStartOver} className="mc-btn-secondary px-5 py-2 rounded-full text-sm mt-1">Back another round</button>
      </div>
    );
  }

  if (flow.kind === 'awaitingWallet') {
    return (
      <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
        <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
        <p className="text-sm mc-text-primary font-bold">Confirm the transfer in your wallet…</p>
        <p className="text-xs mc-text-dim">Approve the {formatSOL(flow.lamports)} SOL transfer in Phantom.</p>
        <button onClick={handleCancelWallet} className="mc-btn-secondary px-4 py-1.5 rounded-full text-xs mt-1">Cancel</button>
      </div>
    );
  }

  if (flow.kind === 'opening') {
    return (
      <div className="mc-card p-6 text-center max-w-md mx-auto space-y-3">
        <Loader2 className="h-8 w-8 mc-text-gold mx-auto animate-spin" />
        <p className="text-sm mc-text-primary font-bold">Registering your Series A position…</p>
        <p className="text-xs mc-text-dim">Your {formatSOL(flow.lamports)} SOL transfer is confirmed — usually registers within a few seconds (up to a minute).</p>
      </div>
    );
  }

  if (flow.kind === 'manual') {
    const qrPayload = `solana:${flow.depositAddress}?amount=${formatSOL(flow.lamports)}`;
    return (
      <div className="mc-card p-4 space-y-3 max-w-md mx-auto">
        {flow.note && (
          <div className="mc-status-red p-2 text-xs text-center">{flow.note}</div>
        )}
        <div className="text-center">
          <div className="mc-label">Send exactly</div>
          <div className="text-2xl font-bold mc-text-gold">{formatSOL(flow.lamports)} SOL</div>
          <div className="text-xs mc-text-dim mt-1">from your wallet — your Series A position registers automatically within ~a minute.</div>
        </div>
        <div className="mc-label">Deposit address</div>
        <div className="flex items-center gap-2">
          <code className="text-xs font-mono mc-text-dim truncate flex-1" title={flow.depositAddress}>{flow.depositAddress}</code>
          <button onClick={() => handleCopy(flow.depositAddress)} className="mc-btn-secondary text-xs">
            {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          </button>
        </div>
        <div className="flex justify-center pt-2">
          <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
        </div>
        <div className="flex gap-2 justify-center pt-2 flex-wrap">
          <button onClick={handleCheckNow} disabled={pokeMut.isPending} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">
            {pokeMut.isPending ? 'Checking…' : 'Check now'}
          </button>
          <button onClick={handleStartOver} className="mc-btn-secondary px-4 py-2 rounded-full text-sm">Start over</button>
        </div>
      </div>
    );
  }

  // input (default)
  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      {/* Left: copy + amount + deposit */}
      <div className="space-y-3">
        <p className="text-sm mc-text-dim">
          Back the next generation of yield innovation. Earn your entitlement — you've earned it.
        </p>

        <div className="flex justify-between items-center">
          <span className="mc-label">Amount</span>
          <span className="text-xs mc-text-dim">SOL from your Phantom wallet</span>
        </div>

        <div className="flex gap-2">
          <button onClick={() => setSolInput(String(MIN_BACKER_SOL))} className="mc-btn-secondary px-3 py-1 text-xs rounded-lg whitespace-nowrap">MIN</button>
          <input
            type="text"
            inputMode="decimal"
            value={solInput}
            onChange={(e) => setSolInput(e.target.value)}
            placeholder={`Min: ${MIN_BACKER_SOL} SOL`}
            className="mc-input flex-1 text-center [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
          />
        </div>

        {belowMin && (<div className="text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Minimum Series A backing is {MIN_BACKER_SOL} SOL</div>)}
        {!solanaPubkey && (<div className="text-xs mc-text-danger"><AlertTriangle className="h-3 w-3 inline mr-1" />Reconnect your Solana wallet to deposit.</div>)}

        <button
          onClick={handleOneClick}
          disabled={!canDeposit}
          className="mc-btn-primary w-full py-2 text-sm font-bold inline-flex items-center justify-center gap-2"
        >
          {prepareMut.isPending ? 'Starting…' : <><Gem className="h-4 w-4" /> Back with Phantom</>}
        </button>

        {prepareMut.isError && (<p className="text-xs mc-text-danger text-center">{(prepareMut.error as Error).message}</p>)}

        <button onClick={handleManual} disabled={!canDeposit} className="w-full text-xs mc-text-muted hover:mc-text-primary transition-colors underline">
          or get a deposit address to send manually
        </button>

        <div className="space-y-1">
          <p className="text-xs mc-text-green font-bold">Series A backers get a guaranteed* 24% return on their capital + {ppPerSol.toLocaleString()} PP per SOL.</p>
          <p className="text-xs mc-text-muted italic">*(Returns not guaranteed)</p>
        </div>
      </div>

      {/* Right: instant calculator + warning */}
      <div className="flex flex-col gap-3 lg:h-full">
        {solFloat > 0 && (
          <div>
            <div className="text-center mb-3">
              <span className="text-xs font-bold mc-text-primary">Expected Series A Return</span>
            </div>
            <div className="mc-card p-4 space-y-3">
              <div className="flex justify-between items-center">
                <div>
                  <div className="mc-label">Total Payout</div>
                  <div className="text-xl font-bold mc-text-green">{formatSolFloat(expectedReturn)} SOL</div>
                  <div className="text-xs mc-text-green opacity-70">24% bonus</div>
                </div>
                <div className="text-right">
                  <div className="mc-label">Ponzi Points</div>
                  <div className="text-xl font-bold mc-text-purple mc-glow-purple">{projectedPP.toLocaleString()}</div>
                  <div className="text-xs mc-text-purple opacity-70">{ppPerSol.toLocaleString()} / SOL</div>
                </div>
              </div>
              <div className="border-t border-white/10 pt-3 space-y-1.5">
                <div className="flex justify-between text-sm">
                  <span className="mc-text-muted">Net deposit (no fees)</span>
                  <span className="mc-text-primary font-medium">{formatSolFloat(solFloat)} SOL</span>
                </div>
                <div className="flex items-center justify-between pt-1">
                  <div className="flex items-center gap-1.5">
                    <TrendingUp className="h-3.5 w-3.5 mc-text-cyan" />
                    <span className="text-xs mc-text-dim">Bonus earned</span>
                  </div>
                  <span className="text-sm font-bold mc-text-cyan">+{formatSolFloat(expectedReturn - solFloat)} SOL</span>
                </div>
              </div>
            </div>
          </div>
        )}
        <div className="mc-status-red p-3 text-center text-sm font-bold rounded-lg lg:mt-auto">
          <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME
          <div className="font-normal text-xs opacity-80 mt-0.5">Only play with money you can afford to lose</div>
        </div>
      </div>
    </div>
  );
}
