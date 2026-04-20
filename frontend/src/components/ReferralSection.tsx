import React, { useState, useRef } from 'react';
import { useGetReferralStats, useGetPonziPoints } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Copy, Check, Share2, ExternalLink, Award, Download, Dice5, Globe } from 'lucide-react';
import type { TabType } from '../App';
import { QRCodeCanvas } from 'qrcode.react';
import { REFERRAL_L1_RATE, REFERRAL_L2_RATE, REFERRAL_L3_RATE, pct } from '../lib/gameConstants';


interface ReferralSectionProps {
  onTabChange?: (tab: TabType) => void;
}

export default function ReferralSection({ onTabChange }: ReferralSectionProps) {
  const [copied, setCopied] = useState(false);
  const [showQR, setShowQR] = useState(false);
  const qrRef = useRef<HTMLCanvasElement>(null);
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

  const downloadQR = () => {
    const canvas = qrRef.current;
    if (!canvas) return;
    const url = canvas.toDataURL('image/png');
    const link = document.createElement('a');
    link.href = url;
    link.download = 'musical-chairs-referral-qr.png';
    link.click();
  };

  if (isLoading) return <LoadingSpinner />;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Tagline */}
        <div className="text-center mb-6">
          <span className="font-accent text-2xl mc-text-gold">More Than Just a Ponzi</span>
          <br />
          <span className="font-accent text-xl mc-text-gold opacity-80">It's Also a Pyramid Scheme!</span>
        </div>

        {/* Referral link + Share buttons — compact layout */}
        <div className="mb-6">
          <div className="mc-label mb-2">Your Referral Link</div>
          <div className="flex gap-2 mb-3">
            <div className="mc-card flex-1 p-3 text-xs mc-text-dim truncate font-mono min-w-0">
              {referralLink}
            </div>
            <button onClick={copyToClipboard} className="mc-btn-pill flex items-center gap-1 whitespace-nowrap">
              {copied ? <><Check className="h-4 w-4" /> Copied!</> : <><Copy className="h-4 w-4" /> Copy</>}
            </button>
          </div>

          {/* Share buttons — inline with socials */}
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs mc-text-muted font-bold">Share:</span>
            {(() => {
              const shareText = "I joined the Musical Chairs Ponzi. Come get stuck in with me before the music stops. 🪑";
              return (
                <>
                  <button
                    onClick={() => window.open(`https://twitter.com/intent/tweet?text=${encodeURIComponent(shareText)}&url=${encodeURIComponent(referralLink)}`, '_blank')}
                    className="mc-btn-secondary flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg"
                  >
                    <span>𝕏</span> Twitter
                  </button>
                  <button
                    onClick={() => window.open(`https://t.me/share/url?url=${encodeURIComponent(referralLink)}&text=${encodeURIComponent(shareText)}`, '_blank')}
                    className="mc-btn-secondary flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg"
                  >
                    <ExternalLink className="h-3 w-3" /> Telegram
                  </button>
                  <button
                    onClick={() => window.open(`https://wa.me/?text=${encodeURIComponent(shareText + " " + referralLink)}`, '_blank')}
                    className="mc-btn-secondary flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg"
                  >
                    <Share2 className="h-3 w-3" /> WhatsApp
                  </button>
                </>
              );
            })()}
            <a
              href={`https://www.facebook.com/sharer/sharer.php?u=${encodeURIComponent(referralLink)}`}
              target="_blank"
              rel="noopener noreferrer"
              className="mc-btn-secondary flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg"
            >
              <Globe className="h-3 w-3" /> Facebook
            </a>
            <button
              onClick={() => setShowQR(!showQR)}
              className="mc-btn-secondary flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg"
            >
              {showQR ? 'Hide QR' : 'QR Code'}
            </button>
          </div>

          {/* QR Code — hidden by default */}
          {showQR && (
            <div className="flex flex-col items-center mt-4 p-4 mc-card">
              <QRCodeCanvas ref={qrRef} value={referralLink} size={160} bgColor="#0a0812" fgColor="#ffffff" level="M" />
              <p className="text-xs mc-text-muted mt-2">Scan to join your pyramid</p>
              <button onClick={downloadQR} className="mc-btn-secondary text-xs mt-2 flex items-center gap-1.5">
                <Download className="h-3.5 w-3.5" /> Download QR
              </button>
            </div>
          )}
        </div>

        {/* Milestones */}
        {(() => {
          const totalRefs = (referralStats?.level1Count || 0);
          const milestones = [
            { count: 1, name: 'First Blood', color: 'mc-text-danger' },
            { count: 5, name: 'Networker', color: 'mc-text-cyan' },
            { count: 10, name: 'Pyramid Architect', color: 'mc-text-gold' },
            { count: 25, name: 'MLM Legend', color: 'mc-text-purple' },
          ];
          return (
            <div className="flex flex-wrap gap-2 mb-6">
              {milestones.map(m => (
                <span key={m.count} className={`inline-flex items-center gap-1 px-3 py-1 rounded-full text-xs font-bold border ${
                  totalRefs >= m.count
                    ? `${m.color} border-current`
                    : 'mc-text-muted border-white/10 opacity-30'
                }`}>
                  <Award className="h-3 w-3" /> {m.name}
                </span>
              ))}
            </div>
          );
        })()}

        {/* Stats */}
        {(() => {
          const directReferrals = referralStats?.level1Count || 0;
          const level2 = referralStats?.level2Count || 0;
          const level3 = referralStats?.level3Count || 0;
          // Per-tier referral PP earnings retired with the backend PP strip;
          // use chip balance as a proxy for "PP available to spend" here.
          const referralPP = ponziData?.chipPoints || 0;
          const directContext =
            directReferrals === 0 ? 'Share your link to get started' :
            directReferrals < 5 ? `${5 - directReferrals} more for Networker badge` :
            directReferrals < 10 ? `${10 - directReferrals} more for Pyramid Architect` :
            directReferrals < 25 ? `${25 - directReferrals} more for MLM Legend` :
            'Top recruiter energy';
          return (
            <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3">
              <div className="mc-card p-3 text-center">
                <div className="mc-label mb-1">Direct Referrals</div>
                <div className="text-xl font-bold mc-text-primary">{directReferrals}</div>
                <div className="text-xs mc-text-dim mt-1">{directContext}</div>
              </div>
              <div className="mc-card p-3 text-center">
                <div className="mc-label mb-1">Level 2</div>
                <div className="text-xl font-bold mc-text-primary">{level2}</div>
                <div className="text-xs mc-text-dim mt-1">Your referrals' referrals</div>
              </div>
              <div className="mc-card p-3 text-center">
                <div className="mc-label mb-1">Level 3</div>
                <div className="text-xl font-bold mc-text-primary">{level3}</div>
                <div className="text-xs mc-text-dim mt-1">Three levels deep</div>
              </div>
              <div className="mc-card p-3 text-center">
                <div className="mc-label mb-1">Chip PP</div>
                <div className="text-xl font-bold mc-text-purple">{referralPP.toLocaleString()}</div>
                <div className="text-xs mc-text-dim mt-1">
                  {referralPP === 0 ? 'Deposit chips to start spending' :
                   referralPP < 100 ? 'Keep growing your network' :
                   `Enough for ${Math.floor(referralPP / 100)} shenanigan casts`}
                </div>
              </div>
            </div>
          );
        })()}

        {/* Spend PP bridge CTA */}
        {(ponziData?.totalPoints || 0) >= 100 && onTabChange && (
          <button
            onClick={() => onTabChange('shenanigans')}
            className="mc-btn-secondary flex items-center gap-2 mx-auto mt-4 text-xs"
          >
            <Dice5 className="h-4 w-4 mc-text-purple" />
            Spend your PP on Shenanigans →
          </button>
        )}
      </div>

      {/* Network visualization / How it works */}
      <div className="mc-card p-5">
        <h3 className="font-display text-base mc-text-primary mb-4">Your Pyramid</h3>
          {/* Visual pyramid tiers */}
          <div className="flex flex-col items-center gap-2 mb-4">
            {/* You — top of pyramid */}
            <div className="flex flex-col items-center">
              <div className="w-10 h-10 rounded-full bg-[var(--mc-gold)]/20 border-2 border-[var(--mc-gold)] flex items-center justify-center text-xs font-bold mc-text-gold">
                You
              </div>
            </div>
            {/* Connector lines */}
            <div className="w-px h-3 bg-white/10" />
            {/* Level 1 */}
            <div className="flex flex-col items-center gap-1">
              <div className="flex items-center gap-2">
                <div className="h-px w-6 bg-[var(--mc-neon-green)]/30" />
                <div className="px-3 py-1.5 rounded-full bg-[var(--mc-neon-green)]/10 border border-[var(--mc-neon-green)]/30 text-xs font-bold mc-text-green">
                  L1 · <span className="mc-text-muted">{pct(REFERRAL_L1_RATE)} of their PP</span> · {referralStats?.level1Points?.toLocaleString() || 0} PP
                </div>
                <div className="h-px w-6 bg-[var(--mc-neon-green)]/30" />
              </div>
              <span className="text-xs mc-text-dim">Direct referrals</span>
            </div>
            <div className="w-px h-3 bg-white/10" />
            {/* Level 2 */}
            <div className="flex flex-col items-center gap-1">
              <div className="flex items-center gap-2">
                <div className="h-px w-10 bg-[var(--mc-cyan)]/30" />
                <div className="px-3 py-1.5 rounded-full bg-[var(--mc-cyan)]/10 border border-[var(--mc-cyan)]/30 text-xs font-bold mc-text-cyan">
                  L2 · <span className="mc-text-muted">{pct(REFERRAL_L2_RATE)} of their PP</span> · {referralStats?.level2Points?.toLocaleString() || 0} PP
                </div>
                <div className="h-px w-10 bg-[var(--mc-cyan)]/30" />
              </div>
              <span className="text-xs mc-text-dim">Your referrals' referrals</span>
            </div>
            <div className="w-px h-3 bg-white/10" />
            {/* Level 3 */}
            <div className="flex flex-col items-center gap-1">
              <div className="flex items-center gap-2">
                <div className="h-px w-14 bg-[var(--mc-purple)]/30" />
                <div className="px-3 py-1.5 rounded-full bg-[var(--mc-purple)]/10 border border-[var(--mc-purple)]/30 text-xs font-bold mc-text-purple">
                  L3 · <span className="mc-text-muted">{pct(REFERRAL_L3_RATE)} of their PP</span> · {referralStats?.level3Points?.toLocaleString() || 0} PP
                </div>
                <div className="h-px w-14 bg-[var(--mc-purple)]/30" />
              </div>
              <span className="text-xs mc-text-dim">Three levels deep</span>
            </div>
          </div>
          <p className="text-xs mc-text-muted text-center italic font-accent">
            We could have hidden the pyramid mechanics, but where's the fun in that?
          </p>
        </div>
    </div>
  );
}
