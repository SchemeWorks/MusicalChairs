import React from 'react';
import { useGetTopPonziPointsHolders, useGetTopPonziPointsBurners } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

interface HallOfFameEntry {
  rank: number;
  name: string;
  ponziPoints?: number;
  ponziPointsBurned?: number;
  principal: string;
}

export default function HallOfFame() {
  const { data: holdersData, isLoading: holdersLoading, error: holdersError } = useGetTopPonziPointsHolders();
  const { data: burnersData, isLoading: burnersLoading, error: burnersError } = useGetTopPonziPointsBurners();

  const isLoading = holdersLoading || burnersLoading;
  const error = holdersError || burnersError;

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🏆 Hall of Fame 🏆
          </h2>
        </div>
        
        <div className="rewards-single-container">
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🏆 Hall of Fame 🏆
          </h2>
        </div>
        
        <div className="rewards-single-container">
          <Card className="border-red-500 bg-red-50">
            <CardContent className="pt-4">
              <p className="text-red-800 font-bold text-center">
                ⚠️ Unable to load Hall of Fame data. Please try again later.
              </p>
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  const getRankStyling = (rank: number) => {
    switch (rank) {
      case 1:
        return {
          cardClass: 'border-4 border-yellow-400 bg-gradient-to-r from-yellow-100 to-yellow-200 shadow-lg',
          badgeClass: 'bg-yellow-500 text-yellow-900',
          emoji: '🥇',
          title: 'Gold'
        };
      case 2:
        return {
          cardClass: 'border-4 border-gray-400 bg-gradient-to-r from-gray-100 to-gray-200 shadow-lg',
          badgeClass: 'bg-gray-500 text-gray-900',
          emoji: '🥈',
          title: 'Silver'
        };
      case 3:
        return {
          cardClass: 'border-4 border-orange-400 bg-gradient-to-r from-orange-100 to-orange-200 shadow-lg',
          badgeClass: 'bg-orange-500 text-orange-900',
          emoji: '🥉',
          title: 'Bronze'
        };
      default:
        return {
          cardClass: 'border-2 border-gray-200 bg-white hover:border-purple-300 hover:shadow-md transition-all',
          badgeClass: 'bg-purple-100 text-purple-800',
          emoji: '🏅',
          title: `#${rank}`
        };
    }
  };

  const renderLeaderboardEntry = (entry: HallOfFameEntry, isHolders: boolean) => {
    const styling = getRankStyling(entry.rank);
    const value = isHolders ? entry.ponziPoints : entry.ponziPointsBurned;
    
    return (
      <Card 
        key={`${isHolders ? 'holder' : 'burner'}-${entry.rank}`} 
        className={styling.cardClass}
      >
        <CardContent className="pt-3 pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <div className="text-xl">{styling.emoji}</div>
              <div>
                <div className="flex items-center space-x-2">
                  <Badge className={styling.badgeClass}>
                    {styling.title}
                  </Badge>
                  <span className="font-black text-sm text-gray-900">
                    {entry.name}
                  </span>
                </div>
              </div>
            </div>
            <div className="text-right">
              <div className="text-lg font-black text-purple-600">
                {value?.toLocaleString() || 0}
              </div>
              <div className="text-xs text-gray-600">
                {isHolders ? 'Points' : 'Burned'}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  };

  const hasData = (holdersData && holdersData.length > 0) || (burnersData && burnersData.length > 0);

  if (!hasData) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🏆 Hall of Fame 🏆
          </h2>
        </div>
        
        <div className="rewards-single-container">
          <div className="text-center py-8">
            <div className="text-6xl mb-4">🏆</div>
            <p className="text-white font-bold text-lg text-with-backdrop">No Ponzi Points activity yet!</p>
            <p className="text-white text-sm mt-2 text-with-backdrop">
              Start playing to earn Ponzi Points and claim your spot on the leaderboard!
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header outside the container */}
      <div className="text-center">
        <h2 className="text-2xl font-black text-white text-with-backdrop">
          🏆 Hall of Fame 🏆
        </h2>
      </div>
      
      {/* Single frosted glass container wrapping all content below header */}
      <div className="rewards-single-container">
        <div className="space-y-6">
          {/* Single unified bubble containing both leaderboards */}
          <Card className="border-4 border-purple-400">
            <CardContent className="pt-6">
              {/* Two-column layout within single bubble */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* Left column: Top Ponzi Points Holders */}
                <div className="space-y-4">
                  <div className="text-center">
                    <h3 className="text-xl font-black text-white text-with-backdrop">
                      Top Ponzi Points Holders
                    </h3>
                    <div className="text-white text-with-backdrop font-semibold text-sm">
                      Most Points Accumulated
                    </div>
                  </div>
                  
                  <div className="space-y-2 max-h-96 overflow-y-auto">
                    {holdersData && holdersData.length > 0 ? (
                      holdersData.map((entry) => renderLeaderboardEntry(entry, true))
                    ) : (
                      <div className="text-center py-4">
                        <p className="text-white text-with-backdrop">No holders yet!</p>
                      </div>
                    )}
                  </div>
                </div>

                {/* Right column: Top Ponzi Points Burners */}
                <div className="space-y-4">
                  <div className="text-center">
                    <h3 className="text-xl font-black text-white text-with-backdrop">
                      Top Ponzi Points Burners
                    </h3>
                    <div className="text-white text-with-backdrop font-semibold text-sm">
                      Most Points Burned on Shenanigans
                    </div>
                  </div>
                  
                  <div className="space-y-2 max-h-96 overflow-y-auto">
                    {burnersData && burnersData.length > 0 ? (
                      burnersData.map((entry) => renderLeaderboardEntry(entry, false))
                    ) : (
                      <div className="text-center py-4">
                        <p className="text-white text-with-backdrop">No burners yet!</p>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Footer Message */}
          <Card className="border-2 border-purple-300 bg-purple-50">
            <CardContent className="pt-4">
              <div className="text-center text-purple-800 font-semibold">
                <div className="text-lg mb-2">🎯 Climb the Leaderboards!</div>
                <div className="text-sm">
                  Earn Ponzi Points by making deposits and referring friends. Burn them on Shenanigans to climb the burners leaderboard!
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
