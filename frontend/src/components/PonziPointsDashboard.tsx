import React from 'react';
import { useGetPonziPoints } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

export default function PonziPointsDashboard() {
  const { data: ponziData, isLoading, error } = useGetPonziPoints();

  if (isLoading) {
    return (
      <div className="rewards-single-container">
        <div className="text-center">
          <h2 className="text-2xl font-black text-center text-white text-with-backdrop mb-2">
            🎯 Ponzi Points Dashboard 🎯
          </h2>
          <p className="text-center text-white text-with-backdrop mb-6">
            Worthless token rewards for YOLOing into a Ponzi
          </p>
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="rewards-single-container">
        <div className="text-center">
          <h2 className="text-2xl font-black text-center text-white text-with-backdrop mb-2">
            🎯 Ponzi Points Dashboard 🎯
          </h2>
          <p className="text-center text-white text-with-backdrop mb-6">
            Worthless token rewards for YOLOing into a Ponzi
          </p>
          <Card className="border-red-500 bg-red-50">
            <CardContent className="pt-4">
              <p className="text-red-800 font-bold text-center">
                ⚠️ Unable to load Ponzi Points data. Please try again later.
              </p>
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  return (
    <div className="rewards-single-container">
      {/* Header */}
      <div className="text-center mb-6">
        <h2 className="text-2xl font-black text-center text-white text-with-backdrop mb-2">
          🎯 Ponzi Points Dashboard 🎯
        </h2>
        <p className="text-center text-white text-with-backdrop">
          Worthless token rewards for YOLOing into a Ponzi
        </p>
      </div>
      
      <div className="space-y-6">
        {/* Consolidated Ponzi Points Display */}
        <Card className="border-4 border-purple-300 bg-gradient-to-r from-purple-100 to-pink-100">
          <CardContent className="pt-6">
            <div className="text-center">
              <div className="text-sm font-bold text-purple-800 mb-2">Total Ponzi Points</div>
              <div className="text-5xl font-black text-purple-900 mb-2">
                {ponziData?.totalPoints.toLocaleString() || '0'}
              </div>
              <div className="text-sm text-purple-700 font-bold mb-3">
                Absolutely worthless but fun to collect!
              </div>
              
              {/* Consolidated breakdown */}
              <div className="grid grid-cols-2 gap-4 mb-3">
                <div className="bg-blue-100 rounded-lg p-3 border border-blue-300">
                  <div className="text-sm font-bold text-blue-800 mb-1">💰 Deposit Points</div>
                  <div className="text-2xl font-black text-blue-900">
                    {ponziData?.depositPoints.toLocaleString() || '0'}
                  </div>
                  <div className="text-xs text-blue-600">From your game deposits</div>
                </div>
                <div className="bg-green-100 rounded-lg p-3 border border-green-300">
                  <div className="text-sm font-bold text-green-800 mb-1">🔗 Referral Points</div>
                  <div className="text-2xl font-black text-green-900">
                    {ponziData?.referralPoints.toLocaleString() || '0'}
                  </div>
                  <div className="text-xs text-green-600">From your passive income network</div>
                </div>
              </div>
              
              <Badge variant="outline" className="text-purple-700 border-purple-400">
                🎪 For Entertainment Only
              </Badge>
            </div>
          </CardContent>
        </Card>

        {/* What are Ponzi Points and Point Multipliers side by side */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* What are Ponzi Points Info Card */}
          <Card className="border-2 border-yellow-400 bg-gradient-to-r from-yellow-100 to-orange-100">
            <CardHeader>
              <CardTitle className="text-lg text-center text-yellow-800">
                🤔 What are Ponzi Points? 🤔
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="text-sm text-yellow-800 font-semibold space-y-2">
                <p className="text-center font-bold text-lg">
                  Tokens as valuable as the promises of a Ponzi scheme!
                </p>
                
                <div className="bg-white rounded-lg p-3 border border-yellow-300">
                  <p className="font-bold text-yellow-900 mb-2">📈 How You Earn Them:</p>
                  <div className="text-xs space-y-1">
                    <p>• Base rate: <strong>1000 Ponzi Points</strong> per 1 ICP deposited</p>
                    <p>• Simple plans get <strong>1x</strong> multiplier</p>
                    <p>• Compounding plans get enhanced multipliers: <strong>2x</strong> for 15-day, <strong>3x</strong> for 30-day</p>
                    <p>• Plus you earn a cut from all your referred users' rewards forever!</p>
                  </div>
                </div>

                <div className="bg-red-100 rounded-lg p-3 border border-red-300">
                  <p className="font-bold text-red-900 mb-2">⚠️ What They're Worth:</p>
                  <ul className="space-y-1 text-xs text-red-800">
                    <li>• Essentially nothing in a monetary sense</li>
                    <li>• Bragging rights as you ascend the leaderboard</li>
                    <li>• Not supported for trading, but nothing's stopping you from making a liquidity pool</li>
                    <li>• May be used to unlock cosmetic features at a later date, but probably not</li>
                  </ul>
                </div>

                <div className="bg-purple-100 rounded-lg p-3 border border-purple-300">
                  <p className="font-bold text-purple-900 mb-2">🎪 The Real Purpose:</p>
                  <ul className="space-y-1 text-xs text-purple-800">
                    <li>• To gamify your gambling addiction</li>
                    <li>• To make you feel a little better about losing money</li>
                    <li>• To create artificial engagement and FOMO</li>
                    <li>• To incentivize the pyramid scheme</li>
                  </ul>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Multiplier Information */}
          <Card className="border-2 border-indigo-400 bg-indigo-50">
            <CardHeader>
              <CardTitle className="text-lg text-center text-indigo-800">
                🎲 Point Multipliers 🎲
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 gap-4">
                <div>
                  <h5 className="font-bold text-indigo-800 mb-2">🌱 Simple Plan Multipliers:</h5>
                  <ul className="text-sm text-indigo-700 space-y-1">
                    <li>🌱 21-Day Plan: 1x multiplier</li>
                  </ul>
                </div>
                <div>
                  <h5 className="font-bold text-indigo-800 mb-2">🔥 Compounding Plan Multipliers:</h5>
                  <ul className="text-sm text-indigo-700 space-y-1">
                    <li>🚀 15-Day Plan: 2x multiplier</li>
                    <li>💎 30-Day Plan: 3x multiplier</li>
                  </ul>
                </div>
                <div>
                  <h5 className="font-bold text-indigo-800 mb-2">🏰 Add House Money Bonus:</h5>
                  <ul className="text-sm text-indigo-700 space-y-1">
                    <li>💰 4,000 Ponzi Points per ICP deposited</li>
                  </ul>
                </div>
              </div>
              <div className="mt-4 bg-indigo-100 rounded-lg p-3 border border-indigo-300">
                <p className="text-xs text-indigo-800 text-center font-semibold">
                  💡 Example: 10 ICP in 30-day compounding = 10 × 1000 × 3 = 30,000 Ponzi Points!
                  <br />
                  <span className="text-indigo-600">(Still worth exactly $0.00)</span>
                </p>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Centered footer text with backdrop styling */}
        <div className="text-center text-white font-bold text-with-backdrop">
          🎭 Ponzi Points: Making gambling feel rewarding since never! 🎭
          <br />
          <span className="text-sm">Collect them for nothing! Feel accomplished about meaningless numbers!</span>
        </div>
      </div>
    </div>
  );
}
