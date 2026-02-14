import React, { useState } from 'react';
import { useGetReferralStats, useGetPonziPoints } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Copy, Check } from 'lucide-react';

export default function ReferralSection() {
  const [copied, setCopied] = useState(false);
  const { data: referralStats, isLoading, error } = useGetReferralStats();
  const { data: ponziData } = useGetPonziPoints();

  const referralLink = referralStats?.referralLink || 'https://musicalchairs.fun/ref/loading...';

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(referralLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Tagline */}
        <div className="text-center mb-6">
          <span className="font-accent text-xl mc-text-gold">More Than Just a Ponzi</span>
          <br />
          <span className="font-accent text-lg mc-text-gold opacity-80">— It's Also a Pyramid Scheme!</span>
        </div>

        {/* Referral link */}
        <div className="mb-6">
          <div className="mc-label mb-2">Your Referral Link</div>
          <div className="flex gap-2">
            <div className="mc-card flex-1 p-3 text-xs mc-text-dim truncate font-mono">
              {referralLink}
            </div>
            <button onClick={copyToClipboard} className="mc-btn-pill flex items-center gap-1 whitespace-nowrap">
              {copied ? <><Check className="h-4 w-4" /> Copied!</> : <><Copy className="h-4 w-4" /> Copy</>}
            </button>
          </div>
        </div>

        {/* Stats */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div className="mc-card p-3 text-center">
            <div className="mc-label mb-1">Direct Referrals</div>
            <div className="text-xl font-bold mc-text-primary">{referralStats?.directReferrals || 0}</div>
          </div>
          <div className="mc-card p-3 text-center">
            <div className="mc-label mb-1">Level 2</div>
            <div className="text-xl font-bold mc-text-primary">{referralStats?.level2Referrals || 0}</div>
          </div>
          <div className="mc-card p-3 text-center">
            <div className="mc-label mb-1">Level 3</div>
            <div className="text-xl font-bold mc-text-primary">{referralStats?.level3Referrals || 0}</div>
          </div>
          <div className="mc-card p-3 text-center">
            <div className="mc-label mb-1">Referral PP</div>
            <div className="text-xl font-bold mc-text-purple">{ponziData?.referralPoints?.toLocaleString() || 0}</div>
          </div>
        </div>
      </div>

      {/* How it works */}
      <div className="mc-card p-5">
        <h3 className="font-display text-base mc-text-primary mb-3">How the Pyramid Works</h3>
        <div className="text-sm mc-text-dim space-y-2 leading-relaxed">
          <p><span className="mc-text-green font-bold">Level 1 (Direct):</span> Earn PP when someone you refer makes a deposit</p>
          <p><span className="mc-text-cyan font-bold">Level 2:</span> Earn PP when your referrals' referrals deposit</p>
          <p><span className="mc-text-purple font-bold">Level 3:</span> It goes deeper. Three levels of MLM glory.</p>
        </div>
        <p className="text-xs mc-text-muted mt-3 italic font-accent">
          We could have hidden the pyramid mechanics, but where's the fun in that?
        </p>
      </div>
    </div>
  );
}
