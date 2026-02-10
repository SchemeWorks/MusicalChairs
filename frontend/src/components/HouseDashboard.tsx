import React, { useState } from 'react';
import { useGetHouseLedger, useGetHouseLedgerStats, useGetDealerPositions, useGetGameStats } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import AddHouseMoney from './AddHouseMoney';
import { formatICP } from '../lib/formatICP';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { AlertTriangle, RefreshCw, Info, DollarSign, TrendingUp, Users, Shield, Zap } from 'lucide-react';
import { Button } from '@/components/ui/button';

// Floating Segmented Tab Control Component
interface SegmentedTabControlProps {
  activeTab: 'dealers' | 'ledger';
  onTabChange: (tab: 'dealers' | 'ledger') => void;
}

function SegmentedTabControl({ activeTab, onTabChange }: SegmentedTabControlProps) {
  const handleKeyDown = (event: React.KeyboardEvent, tab: 'dealers' | 'ledger') => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onTabChange(tab);
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      onTabChange(activeTab === 'dealers' ? 'ledger' : 'dealers');
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      onTabChange(activeTab === 'dealers' ? 'ledger' : 'dealers');
    }
  };

  return (
    <div className="flex justify-center mb-6">
      <div 
        className="inline-flex rounded-full backdrop-blur-sm bg-white/10 border border-white/20 shadow-lg p-1"
        role="tablist"
        aria-label="House Ledger Sections"
      >
        <button
          role="tab"
          aria-selected={activeTab === 'dealers'}
          aria-controls="dealers-panel"
          id="dealers-tab"
          tabIndex={activeTab === 'dealers' ? 0 : -1}
          className={`
            px-6 py-3 rounded-full text-sm font-medium transition-all duration-200 min-h-[44px]
            focus:outline-none focus:ring-2 focus:ring-white/50 focus:ring-offset-2 focus:ring-offset-transparent
            ${activeTab === 'dealers' 
              ? 'bg-green-500/80 text-white shadow-md shadow-green-500/25' 
              : 'text-white/70 hover:text-white/90 hover:bg-white/5 border border-white/10'
            }
          `}
          onClick={() => onTabChange('dealers')}
          onKeyDown={(e) => handleKeyDown(e, 'dealers')}
        >
          Dealer Positions
        </button>
        <button
          role="tab"
          aria-selected={activeTab === 'ledger'}
          aria-controls="ledger-panel"
          id="ledger-tab"
          tabIndex={activeTab === 'ledger' ? 0 : -1}
          className={`
            px-6 py-3 rounded-full text-sm font-medium transition-all duration-200 min-h-[44px]
            focus:outline-none focus:ring-2 focus:ring-2 focus:ring-white/50 focus:ring-offset-2 focus:ring-offset-transparent
            ${activeTab === 'ledger' 
              ? 'bg-purple-500/80 text-white shadow-md shadow-purple-500/25' 
              : 'text-white/70 hover:text-white/90 hover:bg-white/5 border border-white/10'
            }
          `}
          onClick={() => onTabChange('ledger')}
          onKeyDown={(e) => handleKeyDown(e, 'ledger')}
        >
          Ledger Records
        </button>
      </div>
    </div>
  );
}

// Enhanced Error Boundary Component
function ErrorFallback({ error, onRetry, title = "House Ledger Error" }: { error: Error; onRetry: () => void; title?: string }) {
  return (
    <Card className="border-4 border-red-400 bg-gradient-to-r from-red-100 to-orange-100">
      <CardHeader>
        <CardTitle className="text-2xl font-black text-center text-red-800 flex items-center justify-center gap-2">
          <AlertTriangle className="h-8 w-8" />
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="text-center space-y-4">
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription className="text-red-800 font-semibold">
            {error.message || 'Failed to load data. This might be a temporary issue.'}
          </AlertDescription>
        </Alert>
        
        <div className="space-y-2">
          <p className="text-red-700 font-medium">
            Don't worry! Your data is safe. Try refreshing to reload.
          </p>
          <Button 
            onClick={onRetry}
            className="bg-red-600 hover:bg-red-700 text-white font-bold"
          >
            <RefreshCw className="h-4 w-4 mr-2" />
            Retry Loading
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// Consolidated Dealer Info Card Component
function DealerInfoCard() {
  return (
    <Card className="border-2 border-purple-300 bg-gradient-to-br from-purple-50/95 via-pink-50/90 to-blue-50/95 backdrop-blur-sm shadow-xl">
      <CardContent className="pt-6 space-y-6">
        {/* Main Title */}
        <div className="text-center mb-4">
          <h3 className="text-2xl font-black text-purple-800 mb-2">
            🎰 How Dealer Positions Work 🎰
          </h3>
          <p className="text-purple-700 font-semibold text-sm">
            Everything you need to know about becoming a dealer and earning guaranteed returns
          </p>
        </div>

        {/* Grid of Info Sections */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* What Are Dealer Positions */}
          <div className="bg-white/80 rounded-xl p-4 border-2 border-blue-200 hover:border-blue-300 transition-all hover:shadow-md">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 text-3xl">
                <Info className="h-8 w-8 text-blue-600" />
              </div>
              <div>
                <h4 className="font-black text-blue-800 text-lg mb-2">What Are Dealer Positions?</h4>
                <p className="text-sm text-blue-900 leading-relaxed mb-2">
                  There are two types of dealers:
                </p>
                <ul className="text-sm text-blue-900 space-y-1 list-disc list-inside">
                  <li><strong>Upstream Dealers:</strong> Users who voluntarily deposit house money</li>
                  <li><strong>Downstream Dealers:</strong> Users selected by The Redistribution Event</li>
                </ul>
                <p className="text-sm text-blue-900 leading-relaxed mt-2">
                  All dealers are entitled to a <strong>12% return</strong> on their investment plus direct fee payments.
                </p>
              </div>
            </div>
          </div>

          {/* How Repayment Works */}
          <div className="bg-white/80 rounded-xl p-4 border-2 border-green-200 hover:border-green-300 transition-all hover:shadow-md">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 text-3xl">
                <DollarSign className="h-8 w-8 text-green-600" />
              </div>
              <div>
                <h4 className="font-black text-green-800 text-lg mb-2">How Dealer Repayment Works</h4>
                <p className="text-sm text-green-900 leading-relaxed mb-2">
                  Of the 50% of fees earmarked for dealer repayment:
                </p>
                <ul className="text-sm text-green-900 space-y-1 list-disc list-inside">
                  <li><strong>35%</strong> goes to the oldest Upstream Dealer</li>
                  <li><strong>25%</strong> is split evenly among other Upstream Dealers</li>
                  <li><strong>40%</strong> is split evenly among all dealers</li>
                </ul>
              </div>
            </div>
          </div>

          {/* Guaranteed Returns */}
          <div className="bg-white/80 rounded-xl p-4 border-2 border-emerald-200 hover:border-emerald-300 transition-all hover:shadow-md">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 text-3xl">
                <TrendingUp className="h-8 w-8 text-emerald-600" />
              </div>
              <div>
                <h4 className="font-black text-emerald-800 text-lg mb-2">Guaranteed Returns</h4>
                <p className="text-sm text-emerald-900 leading-relaxed">
                  Every ICP you deposit as house money entitles you to <strong>1.12 ICP back (12% bonus)</strong>. This debt is automatically repaid through platform fees, ensuring dealers profit from casino operations.
                </p>
              </div>
            </div>
          </div>

          {/* Risk & Rewards */}
          <div className="bg-white/80 rounded-xl p-4 border-2 border-yellow-200 hover:border-yellow-300 transition-all hover:shadow-md">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 text-3xl">
                <Shield className="h-8 w-8 text-yellow-600" />
              </div>
              <div>
                <h4 className="font-black text-yellow-800 text-lg mb-2">Risk & Rewards</h4>
                <p className="text-sm text-yellow-900 leading-relaxed">
                  While dealer positions offer guaranteed returns, repayment depends on platform activity. More players = faster repayment. You also earn <strong>4,000 Ponzi Points per ICP</strong> deposited!
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* The Redistribution Event - Highlighted Callout */}
        <div className="bg-gradient-to-r from-orange-100 to-red-100 rounded-xl p-5 border-3 border-orange-400 shadow-lg">
          <div className="flex items-start gap-4">
            <div className="flex-shrink-0 text-4xl">🔥</div>
            <div className="flex-1">
              <h4 className="font-black text-orange-900 text-xl mb-3 flex items-center gap-2">
                The Redistribution Event
                <Zap className="h-5 w-5 text-orange-600" />
              </h4>
              <div className="space-y-2 text-sm text-orange-900 leading-relaxed">
                <p>
                  <strong>When the pot empties:</strong> A random unprofitable depositor becomes a new Downstream Dealer automatically!
                </p>
                <p>
                  <strong>New dealer entitlement:</strong> They receive whatever they were underwater during the round, plus a 12% dealer bonus.
                </p>
                <p>
                  <strong>Multiple dealers:</strong> Can coexist and share fee payments according to the updated distribution system.
                </p>
                <p className="text-xs italic mt-3 pt-3 border-t border-orange-300">
                  💡 This system ensures the casino always has backing, even when players drain the pot!
                </p>
              </div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// House Ledger Records Component with enhanced error handling
function HouseLedgerRecords() {
  const { data: ledgerRecords = [], isLoading, error, refetch } = useGetHouseLedger();
  const { data: ledgerStats, isLoading: statsLoading, error: statsError, refetch: refetchStats } = useGetHouseLedgerStats();

  // Handle errors gracefully
  if (error || statsError) {
    return (
      <ErrorFallback 
        error={error || statsError || new Error('Unknown error')} 
        onRetry={() => {
          refetch();
          refetchStats();
        }} 
        title="House Ledger Error"
      />
    );
  }

  if (isLoading || statsLoading) {
    return (
      <Card className="border-4 border-blue-400">
        <CardHeader>
          <CardTitle className="text-2xl font-black text-center text-white text-with-backdrop">
            📊 House Ledger Records
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </CardContent>
      </Card>
    );
  }

  const formatDate = (timestamp: bigint) => {
    try {
      const date = new Date(Number(timestamp) / 1000000); // Convert nanoseconds to milliseconds
      return new Intl.DateTimeFormat('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      }).format(date);
    } catch (error) {
      return 'Invalid Date';
    }
  };

  // Provide default values to prevent crashes
  const safeStats = ledgerStats || {
    totalDeposits: 0,
    totalWithdrawals: 0,
    netBalance: 0,
    recordCount: BigInt(0)
  };

  const safeRecords = Array.isArray(ledgerRecords) ? ledgerRecords : [];

  return (
    <div 
      role="tabpanel" 
      id="ledger-panel" 
      aria-labelledby="ledger-tab"
      className="space-y-6"
    >
      {/* Ledger Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card className="border-2 border-green-400 bg-gradient-to-r from-green-100 to-emerald-100">
          <CardContent className="pt-4 text-center">
            <div className="text-sm font-bold text-green-800 mb-1">Total Deposits</div>
            <div className="text-2xl font-black text-green-900">
              {formatICP(safeStats.totalDeposits)} ICP
            </div>
          </CardContent>
        </Card>

        <Card className="border-2 border-red-400 bg-gradient-to-r from-red-100 to-orange-100">
          <CardContent className="pt-4 text-center">
            <div className="text-sm font-bold text-red-800 mb-1">Total Withdrawals</div>
            <div className="text-2xl font-black text-red-900">
              {formatICP(safeStats.totalWithdrawals)} ICP
            </div>
          </CardContent>
        </Card>

        <Card className="border-2 border-blue-400 bg-gradient-to-r from-blue-100 to-cyan-100">
          <CardContent className="pt-4 text-center">
            <div className="text-sm font-bold text-blue-800 mb-1">Net Balance</div>
            <div className="text-2xl font-black text-blue-900">
              {formatICP(safeStats.netBalance)} ICP
            </div>
          </CardContent>
        </Card>

        <Card className="border-2 border-purple-400 bg-gradient-to-r from-purple-100 to-pink-100">
          <CardContent className="pt-4 text-center">
            <div className="text-sm font-bold text-purple-800 mb-1">Total Records</div>
            <div className="text-2xl font-black text-purple-900">
              {Number(safeStats.recordCount)}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Ledger Records */}
      <Card className="border-4 border-blue-400">
        <CardHeader>
          <CardTitle className="text-2xl font-black text-center text-white text-with-backdrop">
            📊 House Ledger Records
          </CardTitle>
          <CardDescription className="text-center text-gray-600 font-semibold">
            Complete history of all house money transactions
          </CardDescription>
        </CardHeader>
        <CardContent>
          {safeRecords.length === 0 ? (
            <div className="text-center py-8">
              <div className="text-6xl mb-4">📊</div>
              <p className="text-gray-600 font-bold text-lg">No house ledger records yet!</p>
              <p className="text-gray-500 text-sm mt-2">
                House money deposits will appear here.
              </p>
            </div>
          ) : (
            <div className="space-y-3 max-h-96 overflow-y-auto">
              {safeRecords
                .sort((a, b) => Number(b.timestamp) - Number(a.timestamp)) // Sort by newest first
                .map((record) => (
                  <Card
                    key={Number(record.id)}
                    className={`border-2 ${
                      record.amount > 0 
                        ? 'border-green-300 bg-gradient-to-r from-green-50 to-emerald-50'
                        : 'border-red-300 bg-gradient-to-r from-red-50 to-orange-50'
                    }`}
                  >
                    <CardContent className="pt-4">
                      <div className="flex justify-between items-center">
                        <div className="flex items-center gap-3">
                          <div className={`w-10 h-10 rounded-full flex items-center justify-center text-xl ${
                            record.amount > 0 
                              ? 'bg-green-500 text-white'
                              : 'bg-red-500 text-white'
                          }`}>
                            {record.amount > 0 ? '💰' : '💸'}
                          </div>
                          <div>
                            <div className="font-bold text-gray-900">
                              {record.description || 'House Money Transaction'}
                            </div>
                            <div className="text-sm text-gray-600">
                              {formatDate(record.timestamp)}
                            </div>
                          </div>
                        </div>
                        <div className="text-right">
                          <div className={`text-xl font-black ${
                            record.amount > 0 ? 'text-green-600' : 'text-red-600'
                          }`}>
                            {record.amount > 0 ? '+' : ''}{formatICP(record.amount)} ICP
                          </div>
                          <Badge variant={record.amount > 0 ? 'default' : 'destructive'}>
                            Record #{Number(record.id)}
                          </Badge>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// Dealer Positions Component with enhanced error handling and dealer type styling
function DealerPositions() {
  const { data: dealerPositions = [], isLoading, error, refetch } = useGetDealerPositions();
  const { data: gameStats } = useGetGameStats();

  if (error) {
    return (
      <ErrorFallback 
        error={error} 
        onRetry={() => refetch()} 
        title="Dealer Positions Error"
      />
    );
  }

  if (isLoading) {
    return (
      <Card className="border-4 border-yellow-400">
        <CardHeader>
          <CardTitle className="text-2xl font-black text-center text-white text-with-backdrop">
            🏰 Dealers
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </CardContent>
      </Card>
    );
  }

  // Ensure dealerPositions is an array
  const safeDealerPositions = Array.isArray(dealerPositions) ? dealerPositions : [];

  const formatDate = (timestamp: bigint) => {
    try {
      const date = new Date(Number(timestamp) / 1000000); // Convert nanoseconds to milliseconds
      return new Intl.DateTimeFormat('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit'
      }).format(date);
    } catch (error) {
      return 'Invalid Date';
    }
  };

  // Calculate total house money added and outstanding dealer debt
  const totalHouseMoneyAdded = safeDealerPositions.reduce((sum, dealer) => sum + (dealer.amount || 0), 0);
  const totalOutstandingDebt = safeDealerPositions.reduce((sum, dealer) => sum + (dealer.entitlement || 0), 0);

  return (
    <div 
      role="tabpanel" 
      id="dealers-panel" 
      aria-labelledby="dealers-tab"
      className="space-y-6"
    >
      {/* Top Section: Own the Casino and Stacked Stats */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Left Column: Own the Casino + Gambling Warning */}
        <div className="space-y-4">
          <AddHouseMoney />
          
          {/* Gambling Warning - positioned below Own the Casino */}
          <div className="game-setup-warning-red-bubble text-center transition-all duration-300 hover:scale-[1.02]">
            ⚠️ THIS IS A GAMBLING GAME! ⚠️
            <br />
            Only play with money you can afford to lose!
          </div>
        </div>

        {/* Right Column: Stacked Stats as Clean Stat Cards */}
        <div className="flex flex-col justify-center space-y-4">
          {/* Outstanding Dealer Debt - Red Box */}
          <div className="rounded-xl p-6 bg-gradient-to-br from-red-50 to-red-100 border-2 border-red-300 shadow-md">
            <div className="text-center">
              <div className="text-sm font-bold text-red-700 mb-2 uppercase tracking-wide">
                Outstanding Dealer Debt
              </div>
              <div className="text-3xl font-black text-red-900">
                {formatICP(totalOutstandingDebt)} ICP
              </div>
            </div>
          </div>

          {/* Total House Money Added - Blue Box */}
          <div className="rounded-xl p-6 bg-gradient-to-br from-blue-50 to-blue-100 border-2 border-blue-300 shadow-md">
            <div className="text-center">
              <div className="text-sm font-bold text-blue-700 mb-2 uppercase tracking-wide">
                Total House Money Added
              </div>
              <div className="text-3xl font-black text-blue-900">
                {formatICP(totalHouseMoneyAdded)} ICP
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Current Dealers Section */}
      {safeDealerPositions.length > 0 ? (
        <div className="space-y-4">
          <h3 className="text-xl font-black text-white text-with-backdrop text-center">
            Current Dealers
          </h3>
          
          {safeDealerPositions.map((dealer, index) => {
            const repaymentProgress = dealer.entitlement > 0 ? ((dealer.entitlement - dealer.amount) / dealer.entitlement) * 100 : 0;
            const isUpstream = 'upstream' in dealer.dealerType;
            
            return (
              <Card
                key={dealer.owner.toString()}
                className={`border-2 ${
                  isUpstream 
                    ? 'border-emerald-400 bg-gradient-to-r from-emerald-50 to-green-50' 
                    : 'border-amber-400 bg-gradient-to-r from-amber-50 to-yellow-50'
                } relative overflow-hidden`}
                style={{
                  boxShadow: isUpstream 
                    ? '0 0 20px rgba(16, 185, 129, 0.3), inset 0 1px 0 rgba(255, 255, 255, 0.2)' 
                    : '0 0 20px rgba(245, 158, 11, 0.3), inset 0 1px 0 rgba(255, 255, 255, 0.2)',
                }}
              >
                {/* Shimmer animation for Upstream Dealers */}
                {isUpstream && (
                  <div className="absolute inset-0 pointer-events-none">
                    <div 
                      className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent"
                      style={{
                        animation: 'shimmer 3s ease-in-out infinite',
                        backgroundSize: '200% 100%',
                      }}
                    />
                  </div>
                )}
                
                {/* Gold trim for Upstream Dealers */}
                {isUpstream && (
                  <div className="absolute top-0 left-0 right-0 h-1 bg-gradient-to-r from-yellow-400 via-yellow-500 to-yellow-400" />
                )}
                
                {/* Purple border glow for Downstream Dealers */}
                {!isUpstream && (
                  <div 
                    className="absolute inset-0 pointer-events-none rounded-lg"
                    style={{
                      boxShadow: '0 0 15px rgba(168, 85, 247, 0.4)',
                    }}
                  />
                )}
                
                <CardContent className="pt-6 relative z-10">
                  <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                    {/* Dealer Info */}
                    <div className="flex items-center">
                      <div className={`w-12 h-12 rounded-xl flex items-center justify-center text-2xl mr-4 ${
                        isUpstream 
                          ? 'bg-gradient-to-br from-emerald-400 to-green-500' 
                          : 'bg-gradient-to-br from-amber-400 to-yellow-500'
                      }`}>
                        {isUpstream ? '💎' : '🎲'}
                      </div>
                      <div>
                        <div className="font-black text-gray-900 text-lg">{dealer.name}</div>
                        <div className="flex gap-2 mt-1 flex-wrap">
                          <Badge 
                            variant="outline" 
                            className={isUpstream ? 'border-emerald-600 text-emerald-700' : 'border-amber-600 text-amber-700'}
                          >
                            {isUpstream ? 'Upstream Dealer' : 'Downstream Dealer'}
                          </Badge>
                          {(dealer.entitlement - dealer.amount) >= dealer.entitlement && (
                            <Badge variant="default" className="bg-green-600">
                              ✅ Fully Repaid
                            </Badge>
                          )}
                        </div>
                        <div className="text-xs text-gray-500 mt-1">
                          {isUpstream && dealer.firstDepositDate && (
                            <>
                              First Deposit: {formatDate(dealer.firstDepositDate)}
                              <br />
                            </>
                          )}
                          Appointed: {formatDate(dealer.startTime)}
                          <br />
                          Principal: 
                          <br />
                          {dealer.owner.toString()}
                        </div>
                      </div>
                    </div>

                    {/* Entitlement Details */}
                    <div className="text-center lg:text-left">
                      <div className="text-sm font-bold text-gray-600 mb-1">Total Entitlement</div>
                      <div className={`text-2xl font-black ${
                        isUpstream ? 'text-emerald-600' : 'text-amber-600'
                      }`}>
                        {formatICP(dealer.entitlement)} ICP
                      </div>
                    </div>

                    {/* Repayment Status */}
                    <div className="text-center lg:text-left">
                      <div className="text-sm font-bold text-gray-600 mb-1">Repayment Status</div>
                      <div className="text-lg font-black text-blue-600 mb-2">
                        {formatICP(dealer.entitlement - dealer.amount)} / {formatICP(dealer.entitlement)} ICP
                      </div>
                      <Progress value={Math.max(0, Math.min(100, repaymentProgress))} className="mb-2" />
                      <div className="text-xs text-gray-500">
                        {Math.max(0, repaymentProgress).toFixed(1)}% repaid
                      </div>
                      <div className="text-sm font-bold text-red-600 mt-1">
                        Remaining: {formatICP(dealer.amount)} ICP
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      ) : (
        <Card className="border-4 border-yellow-400">
          <CardContent className="pt-6">
            <div className="text-center py-8">
              <div className="text-6xl mb-4">🎰</div>
              <p className="text-gray-600 font-bold text-lg">No dealers yet!</p>
              <p className="text-gray-500 text-sm mt-2">
                Deposit house money above to become the first dealer and start earning guaranteed returns!
              </p>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Consolidated Dealer Info Card */}
      <DealerInfoCard />
    </div>
  );
}

export default function HouseDashboard() {
  const [activeTab, setActiveTab] = useState<'dealers' | 'ledger'>('dealers');

  return (
    <div className="house-ledger-single-container">
      {/* House Ledger Header */}
      <div className="text-center space-y-2">
        <h2 className="text-2xl font-black text-white drop-shadow-md">
          🏰 Dealers
        </h2>
      </div>

      {/* Floating Segmented Tab Control */}
      <SegmentedTabControl activeTab={activeTab} onTabChange={setActiveTab} />

      {/* Tab Content */}
      <div className="transition-all duration-200">
        {activeTab === 'dealers' && <DealerPositions />}
        {activeTab === 'ledger' && <HouseLedgerRecords />}
      </div>
    </div>
  );
}
