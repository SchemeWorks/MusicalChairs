import React, { useMemo, useState } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { Check, Copy } from 'lucide-react';
import {
  usePrepareSolDeposit,
  useGetMyDepositAddress,
  useGetMyPendingSolIntents,
} from '../../hooks/useQueries';
import { formatSOL, parseSOL, LAMPORTS_PER_SOL } from '../../solana/lamports';
import {
  PP_PER_SOL_SIMPLE,
  PP_PER_SOL_COMPOUND_15,
  PP_PER_SOL_COMPOUND_30,
} from '../../lib/gameConstants';
import { SolGamePlan } from '../../backend';

type PlanKey = 'simple21Day' | 'compounding15Day' | 'compounding30Day';

const PLAN_LABELS: Record<PlanKey, string> = {
  simple21Day: 'Simple 21-Day Plan',
  compounding15Day: 'Compounding 15-Day Plan',
  compounding30Day: 'Compounding 30-Day Plan',
};

const PP_PER_SOL: Record<PlanKey, number> = {
  simple21Day: PP_PER_SOL_SIMPLE,
  compounding15Day: PP_PER_SOL_COMPOUND_15,
  compounding30Day: PP_PER_SOL_COMPOUND_30,
};

interface Props {
  onClose?: () => void;
  variant?: 'widget' | 'sheet';
}

export default function BuySOLFlyout({ onClose, variant = 'widget' }: Props) {
  const [planKey, setPlanKey] = useState<PlanKey>('simple21Day');
  const [solInput, setSolInput] = useState('');
  const [copied, setCopied] = useState(false);
  const [intentResult, setIntentResult] = useState<{ intentId: bigint; depositAddress: string } | null>(null);

  const prepareMut = usePrepareSolDeposit();
  const { data: existingAddress } = useGetMyDepositAddress();
  const { data: pendingIntents } = useGetMyPendingSolIntents();

  const lamports = useMemo(() => {
    try {
      return solInput.trim() ? parseSOL(solInput) : 0n;
    } catch {
      return 0n;
    }
  }, [solInput]);

  const projectedPP = useMemo(() => {
    if (lamports === 0n) return 0;
    const solFloat = Number(lamports) / Number(LAMPORTS_PER_SOL);
    return Math.round(solFloat * PP_PER_SOL[planKey]);
  }, [lamports, planKey]);

  const handlePrepare = async () => {
    if (lamports === 0n) return;
    try {
      const result = await prepareMut.mutateAsync({
        plan: SolGamePlan[planKey],
        expectedAmountLamports: lamports,
      });
      setIntentResult({ intentId: result.intentId, depositAddress: result.depositAddress });
    } catch {
      // Error surfaces via prepareMut.isError below.
    }
  };

  const addressToShow = intentResult?.depositAddress ?? existingAddress;
  const qrPayload = addressToShow && lamports > 0n
    ? `solana:${addressToShow}?amount=${formatSOL(lamports)}`
    : addressToShow
      ? `solana:${addressToShow}`
      : null;

  const handleCopy = async () => {
    if (!addressToShow) return;
    await navigator.clipboard.writeText(addressToShow);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={`mc-card-elevated ${variant === 'sheet' ? 'p-6' : 'p-4'}`}>
      <h2 className="font-display text-lg mc-text-primary mb-3">Buy Ponzi Points with SOL</h2>

      <div className="mc-status-amber p-3 mb-4 text-xs font-bold">
        Send mainnet SOL only. Deposits are detected automatically within ~60 seconds and credited to your selected plan.
      </div>

      <label className="block mb-2">
        <span className="mc-label">Plan</span>
        <select
          value={planKey}
          onChange={(e) => setPlanKey(e.target.value as PlanKey)}
          className="mc-select w-full mt-1"
        >
          {Object.entries(PLAN_LABELS).map(([k, v]) => (
            <option key={k} value={k}>{v}</option>
          ))}
        </select>
      </label>

      <label className="block mb-3">
        <span className="mc-label">Amount (SOL)</span>
        <input
          type="text"
          inputMode="decimal"
          value={solInput}
          onChange={(e) => setSolInput(e.target.value)}
          className="mc-input w-full mt-1 font-mono"
          placeholder="0.0"
        />
      </label>

      {projectedPP > 0 && (
        <div className="text-xs mc-text-muted mb-3">
          Projected PP: <span className="mc-text-gold font-bold">{projectedPP.toLocaleString()}</span>
        </div>
      )}

      <button
        onClick={handlePrepare}
        disabled={prepareMut.isPending || lamports === 0n}
        className="mc-btn-primary w-full mb-3"
      >
        {prepareMut.isPending ? 'Preparing…' : 'Get deposit address'}
      </button>

      {prepareMut.isError && (
        <div className="mc-status-red p-2 text-xs mb-3">{(prepareMut.error as Error).message}</div>
      )}

      {addressToShow && (
        <div className="mc-card p-3 space-y-2">
          <div className="mc-label">Send SOL to this address</div>
          <div className="flex items-center gap-2">
            <code className="text-xs font-mono mc-text-dim truncate flex-1" title={addressToShow}>
              {addressToShow}
            </code>
            <button onClick={handleCopy} className="mc-btn-secondary text-xs">
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </div>

          {qrPayload && (
            <div className="flex justify-center pt-2">
              <QRCodeCanvas value={qrPayload} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
            </div>
          )}

          {intentResult && (
            <div className="text-[10px] mc-text-muted text-center">
              intentId: {intentResult.intentId.toString()}
            </div>
          )}
        </div>
      )}

      {pendingIntents && pendingIntents.length > 0 && (
        <div className="text-[10px] mc-text-muted mt-3">
          {pendingIntents.length} pending deposit{pendingIntents.length === 1 ? '' : 's'} awaiting confirmation
        </div>
      )}

      {onClose && (
        <button onClick={onClose} className="mc-btn-secondary w-full mt-3">Close</button>
      )}
    </div>
  );
}
