import React, { useState, useRef, useMemo } from 'react';
import {
  useGetReferralStats,
  useGetPonziPoints,
  useGetMyReferralCode,
  useGetUplineChain,
  useGetUserNames,
} from '../hooks/useQueries';
import { useWallet } from '../hooks/useWallet';
import { isCharles } from '../lib/charles';
import LoadingSpinner from './LoadingSpinner';
import { Copy, Check, Share2, ExternalLink, Award, Download, Dice5, Globe } from 'lucide-react';
import type { TabType } from '../App';
import { QRCodeCanvas } from 'qrcode.react';


interface ReferralSectionProps {
  onTabChange?: (tab: TabType) => void;
}

// Compact display name: real profile name if set, else short principal.
function displayFor(principal: string, nameMap?: Map<string, string>): string {
  const name = nameMap?.get(principal);
  if (name && name.length > 0) return name;
  return `${principal.slice(0, 5)}…${principal.slice(-3)}`;
}

// Color theme per tier — keeps the pyramid readable at a glance.
const TIER_THEME: Record<number, { text: string; ring: string; bg: string; label: string }> = {
  1: { text: 'var(--mc-neon-green)', ring: 'rgba(57, 255, 20, 0.6)', bg: 'rgba(57, 255, 20, 0.08)', label: 'L1' },
  2: { text: 'var(--mc-cyan)', ring: 'rgba(34, 211, 238, 0.6)', bg: 'rgba(34, 211, 238, 0.08)', label: 'L2' },
  3: { text: 'var(--mc-purple)', ring: 'rgba(168, 85, 247, 0.6)', bg: 'rgba(168, 85, 247, 0.08)', label: 'L3' },
};

function PersonNode({
  principal,
  nameMap,
  tier,
  size = 'sm',
}: {
  principal: string;
  nameMap?: Map<string, string>;
  tier: 1 | 2 | 3;
  size?: 'sm' | 'md';
}) {
  const theme = TIER_THEME[tier];
  const label = displayFor(principal, nameMap);
  const pad = size === 'md' ? 'px-3 py-1.5' : 'px-2.5 py-1';
  const [justCopied, setJustCopied] = useState(false);

  // Click-to-copy lets the referrer grab a fresh signup's principal the
  // moment it shows up — the main reason to look at this list is "send
  // my friend some ICP," and reading a tooltip character-by-character
  // was the friction point.
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(principal);
      setJustCopied(true);
      setTimeout(() => setJustCopied(false), 1200);
    } catch {
      // Clipboard may be unavailable (insecure context); fail silently.
    }
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={justCopied ? 'Copied' : `Click to copy principal\n${principal}`}
      className={`inline-flex items-center gap-1.5 rounded-full ${pad} text-xs font-bold whitespace-nowrap hover:brightness-125 active:scale-95 transition`}
      style={{
        background: theme.bg,
        border: `1px solid ${theme.ring}`,
        color: theme.text,
      }}
    >
      <span
        className="inline-block rounded-full"
        style={{ width: 6, height: 6, background: theme.text, boxShadow: `0 0 6px ${theme.ring}` }}
      />
      <span className="truncate max-w-[120px]">{justCopied ? 'copied' : label}</span>
      {justCopied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3 opacity-50" />}
    </button>
  );
}

export default function ReferralSection({ onTabChange }: ReferralSectionProps) {
  const [copied, setCopied] = useState(false);
  const [showQR, setShowQR] = useState(false);
  const qrRef = useRef<HTMLCanvasElement>(null);
  const { data: referralStats, isLoading } = useGetReferralStats();
  const { data: ponziData } = useGetPonziPoints();
  const { data: referralCodeData } = useGetMyReferralCode();
  const { data: uplineChain } = useGetUplineChain(1);

  const referralLink = referralCodeData?.link || referralStats?.referralLink || 'https://musicalchairs.fun/';

  // Charles sits at the top of the system by definition — never show a sponsor
  // above him, even if a stale referralChain entry exists from earlier testing.
  const { principal } = useWallet();
  const viewerIsCharles = !!principal && isCharles(principal);
  const sponsor = viewerIsCharles ? undefined : uplineChain?.[0];

  // Collect every principal we want a name for (sponsor + downline).
  const allPrincipals = useMemo(() => {
    const set = new Set<string>();
    if (sponsor) set.add(sponsor);
    (referralStats?.recentSignups ?? []).forEach((s) => set.add(s.principal));
    return Array.from(set);
  }, [sponsor, referralStats?.recentSignups]);
  const { data: nameMap } = useGetUserNames(allPrincipals);

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

  const directReferrals = referralStats?.level1Count || 0;
  const level2 = referralStats?.level2Count || 0;
  const level3 = referralStats?.level3Count || 0;
  const referralPP = ponziData?.chipPoints || 0;

  // Bucket downline by level — caps shown per row so the layout stays clean.
  const signups = referralStats?.recentSignups ?? [];
  const l1Nodes = signups.filter((s) => s.level === 1);
  const l2Nodes = signups.filter((s) => s.level === 2);
  const l3Nodes = signups.filter((s) => s.level === 3);

  const milestones = [
    { count: 1, name: 'First Blood', color: 'mc-text-danger' },
    { count: 5, name: 'Networker', color: 'mc-text-cyan' },
    { count: 10, name: 'Pyramid Architect', color: 'mc-text-gold' },
    { count: 25, name: 'MLM Legend', color: 'mc-text-purple' },
  ];

  const hasDownline = l1Nodes.length + l2Nodes.length + l3Nodes.length > 0;

  return (
    <div className="space-y-6">
      <div className="mc-card-elevated">
        {/* Tagline */}
        <div className="text-center mb-6">
          <span className="font-accent text-2xl mc-text-gold">More Than Just a Ponzi</span>
          <br />
          <span className="font-accent text-xl mc-text-gold opacity-80">It's Also a Pyramid Scheme!</span>
        </div>

        {/* Referral link + Share buttons */}
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
        <div className="flex flex-wrap gap-2 mb-6">
          {milestones.map((m) => (
            <span
              key={m.count}
              className={`inline-flex items-center gap-1 px-3 py-1 rounded-full text-xs font-bold border ${
                directReferrals >= m.count
                  ? `${m.color} border-current`
                  : 'mc-text-muted border-white/10 opacity-30'
              }`}
            >
              <Award className="h-3 w-3" /> {m.name}
            </span>
          ))}
        </div>

        {/* The Pyramid — sponsor above, You in the middle, downline below */}
        <div className="mc-card p-5 mb-6">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-display text-base mc-text-primary">Your Pyramid</h3>
            <span className="text-xs mc-text-muted italic font-accent">Compounding while you sleep.</span>
          </div>

          <div className="flex flex-col items-center gap-2">
            {/* Sponsor — the one person above you */}
            {sponsor ? (
              <>
                <span className="text-[10px] mc-text-muted uppercase tracking-wider">Your Sponsor</span>
                <div
                  title={sponsor}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-bold whitespace-nowrap"
                  style={{
                    background: 'rgba(255,255,255,0.04)',
                    border: '1px solid rgba(255,255,255,0.2)',
                    color: 'rgba(255,255,255,0.75)',
                  }}
                >
                  <span className="truncate max-w-[180px]">{displayFor(sponsor, nameMap)}</span>
                </div>
                <div className="w-px h-3 bg-white/15" />
              </>
            ) : (
              <>
                <div className="text-xs mc-text-muted italic">No sponsor. You're at the top of your chain.</div>
                <div className="w-px h-3 bg-white/10" />
              </>
            )}

            {/* You */}
            <div
              className="w-14 h-14 rounded-full flex items-center justify-center text-sm font-bold mc-text-gold"
              style={{
                background: 'rgba(255, 215, 0, 0.12)',
                border: '2px solid var(--mc-gold)',
                boxShadow: '0 0 18px rgba(255, 215, 0, 0.35)',
              }}
            >
              You
            </div>

            {/* Branching connector under You */}
            {hasDownline && <div className="w-px h-3 bg-white/15" />}

            {/* Downline tiers */}
            {hasDownline ? (
              <div className="w-full space-y-4 mt-1">
                <DownlineRow
                  tier={1}
                  count={directReferrals}
                  nodes={l1Nodes.map((n) => n.principal)}
                  nameMap={nameMap}
                />
                <DownlineRow
                  tier={2}
                  count={level2}
                  nodes={l2Nodes.map((n) => n.principal)}
                  nameMap={nameMap}
                />
                <DownlineRow
                  tier={3}
                  count={level3}
                  nodes={l3Nodes.map((n) => n.principal)}
                  nameMap={nameMap}
                />
              </div>
            ) : (
              <div className="mt-3 text-xs mc-text-muted italic">
                Share your link to start building your downline.
              </div>
            )}
          </div>
        </div>

        {/* Spend PP CTA */}
        {(ponziData?.totalPoints || 0) >= 100 && onTabChange && (
          <div className="flex justify-center">
            <button
              onClick={() => onTabChange('shenanigans')}
              className="group inline-flex items-center gap-2 px-5 py-2 rounded-full font-bold text-sm transition-all hover:scale-[1.03]"
              style={{
                background: 'rgba(0, 0, 0, 0.6)',
                color: 'var(--mc-purple)',
                border: '1px solid var(--mc-purple)',
                boxShadow: '0 0 10px rgba(168, 85, 247, 0.35)',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.boxShadow = '0 0 20px rgba(168, 85, 247, 0.55)';
                e.currentTarget.style.background = 'rgba(168, 85, 247, 0.1)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.boxShadow = '0 0 10px rgba(168, 85, 247, 0.35)';
                e.currentTarget.style.background = 'rgba(0, 0, 0, 0.6)';
              }}
            >
              <Dice5 className="h-4 w-4" />
              <span>Spend your PP on Shenanigans</span>
              <span aria-hidden className="group-hover:translate-x-0.5 transition-transform">→</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function DownlineRow({
  tier,
  count,
  nodes,
  nameMap,
}: {
  tier: 1 | 2 | 3;
  count: number;
  nodes: string[];
  nameMap?: Map<string, string>;
}) {
  const theme = TIER_THEME[tier];
  const hidden = Math.max(0, count - nodes.length);
  return (
    <div className="flex flex-col items-center gap-2">
      <span
        className="text-[10px] uppercase tracking-wider font-bold"
        style={{ color: theme.text }}
      >
        {theme.label} · {count} {count === 1 ? 'person' : 'people'}
      </span>
      {nodes.length > 0 ? (
        <div className="flex flex-wrap justify-center gap-1.5 max-w-full">
          {nodes.map((p) => (
            <PersonNode key={p} principal={p} nameMap={nameMap} tier={tier} />
          ))}
          {hidden > 0 && (
            <span
              className="inline-flex items-center px-2.5 py-1 rounded-full text-xs font-bold"
              style={{
                background: theme.bg,
                border: `1px dashed ${theme.ring}`,
                color: theme.text,
                opacity: 0.7,
              }}
            >
              +{hidden} more
            </span>
          )}
        </div>
      ) : count > 0 ? (
        <span className="text-xs mc-text-dim italic">{count} {count === 1 ? 'person' : 'people'} — names loading…</span>
      ) : (
        <span className="text-xs mc-text-muted opacity-50">— empty —</span>
      )}
    </div>
  );
}
