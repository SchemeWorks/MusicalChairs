import React, { useState } from 'react';
import { useGetReferralStats, useGetPonziPoints } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';

export default function ReferralSection() {
  const [copied, setCopied] = useState(false);
  const { data: referralStats, isLoading, error } = useGetReferralStats();
  const { data: ponziData } = useGetPonziPoints();

  const referralLink = referralStats?.referralLink || 'https://musical-chairs.com/ref/loading...';

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(referralLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  return (
    <div className="w-full max-w-6xl mx-auto">
      {/* Single animated gradient frosted-glass outer card */}
      <div className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-purple-400/20 via-pink-400/20 to-blue-400/20 backdrop-blur-md border border-white/20 shadow-2xl">
        {/* Animated gradient background */}
        <div className="absolute inset-0 bg-gradient-to-br from-purple-500/10 via-pink-500/10 to-blue-500/10 animate-pulse"></div>
        
        {/* Content container */}
        <div className="relative z-10 p-8 space-y-8">
          {/* Reordered top elements */}
          <div className="space-y-6">
            {/* First: Multi-Level Marketing header */}
            <h2 className="text-2xl font-black text-white drop-shadow-lg text-center">
              🔺 Multi-Level Marketing 🔺
            </h2>

            {/* Second: Tagline - appears only once, directly below the heading */}
            <div className="text-center">
              <div className="pyramid-tagline-horizontal text-yellow-300 text-2xl font-bold animate-bounce">
                <div>More Than Just a Ponzi</div>
                <div>— It's Also a Pyramid Scheme!</div>
              </div>
            </div>

            {/* Build Your Network Section */}
            <div className="space-y-4">
              <h3 className="text-lg text-center text-white drop-shadow-lg font-bold">
                Build Your Network
              </h3>
              
              <div className="text-sm text-white font-semibold text-center">
                <p className="mb-4 font-bold">
                  Build your passive income network and earn Ponzi Points from every recruit's activity perpetually!
                  <br />
                  All you need to do is tell two people, who tell two people, who also tell two people 🤑
                </p>
              </div>

              {/* Referral Link Section */}
              <div className="bg-purple-100/80 backdrop-blur-sm rounded-xl p-4 border border-purple-300/50">
                <div className="text-sm font-bold text-purple-800 text-left mb-3">Your Referral Link</div>
                <div className="flex">
                  <input
                    type="text"
                    value={referralLink}
                    readOnly
                    className="flex-1 px-3 py-2 border-2 border-gray-300 rounded-l-xl bg-gray-50 text-xs font-mono"
                  />
                  <Button
                    onClick={copyToClipboard}
                    disabled={isLoading}
                    className="px-4 py-2 bg-gradient-to-r from-purple-500 to-pink-500 text-white rounded-r-xl hover:from-purple-600 hover:to-pink-600 transition-all font-bold disabled:opacity-50"
                  >
                    {copied ? '✅ Copied!' : '📋 Copy'}
                  </Button>
                </div>
              </div>
            </div>
          </div>

          {/* Consolidated tier cards in top row */}
          <div className="space-y-4">
            {isLoading ? (
              <div className="flex justify-center py-8">
                <LoadingSpinner />
              </div>
            ) : error ? (
              <div className="text-center py-8">
                <p className="text-red-300 font-bold">
                  ⚠️ Unable to load network stats
                </p>
              </div>
            ) : (
              <div className="space-y-6">
                {/* Three tier cards in top row */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  {/* Level 1 Direct */}
                  <div className="bg-green-100/80 backdrop-blur-sm rounded-lg p-4 text-center border border-green-300/50">
                    <div className="text-2xl mb-2">🎯</div>
                    <div className="font-bold text-green-800 mb-1">Level 1 Direct</div>
                    <div className="text-lg font-bold text-green-700">8% Points</div>
                    <div className="text-sm text-green-700 font-semibold">
                      {(referralStats?.level1Points || 0).toLocaleString()} pts earned
                    </div>
                  </div>

                  {/* Level 2 Network */}
                  <div className="bg-blue-100/80 backdrop-blur-sm rounded-lg p-4 text-center border border-blue-300/50">
                    <div className="text-2xl mb-2">🎯</div>
                    <div className="font-bold text-blue-800 mb-1">Level 2 Network</div>
                    <div className="text-lg font-bold text-blue-700">5% Points</div>
                    <div className="text-sm text-blue-700 font-semibold">
                      {(referralStats?.level2Points || 0).toLocaleString()} pts earned
                    </div>
                  </div>

                  {/* Level 3+ Network */}
                  <div className="bg-purple-100/80 backdrop-blur-sm rounded-lg p-4 text-center border border-purple-300/50">
                    <div className="text-2xl mb-2">🎯</div>
                    <div className="font-bold text-purple-800 mb-1">Level 3+ Network</div>
                    <div className="text-lg font-bold text-purple-700">2% Points</div>
                    <div className="text-sm text-purple-700 font-semibold">
                      {(referralStats?.level3Points || 0).toLocaleString()} pts earned
                    </div>
                  </div>
                </div>

                {/* Renamed Total Ponzi Points Earned - prominently styled */}
                <div className="text-center bg-gradient-to-r from-purple-200/80 to-pink-200/80 backdrop-blur-sm rounded-xl p-6 border border-purple-300/50">
                  <div className="text-lg font-bold text-purple-800 mb-2">Total Ponzi Points Earned</div>
                  <div className="text-4xl font-black text-purple-900 drop-shadow-md">
                    {(ponziData?.referralPoints || 0).toLocaleString()}
                  </div>
                  <div className="text-sm text-purple-700 mt-2">
                    From your perpetual passive income network
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Perpetual Passive Income Strategies */}
          <div className="bg-gradient-to-r from-indigo-100/80 to-purple-100/80 backdrop-blur-sm rounded-xl p-6 border border-indigo-300/50">
            <h5 className="font-black text-indigo-900 mb-4 text-center text-xl">🌟 Perpetual Passive Income Strategies 🌟</h5>
            <div className="space-y-3 text-sm text-indigo-800 font-semibold">
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">💎</span>
                <span>Share your link to build direct connections</span>
              </div>
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">🚀</span>
                <span>Teach your network to recruit others</span>
              </div>
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">🔥</span>
                <span>Earn from unlimited network depth forever</span>
              </div>
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">⚡</span>
                <span>Generate passive income perpetually while you sleep</span>
              </div>
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">♾️</span>
                <span>No limits - earn as long as your network participates</span>
              </div>
              <div className="flex items-center">
                <span className="text-indigo-500 mr-3 text-lg">🎪</span>
                <span>Feel accomplished about nothing indefinitely</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
