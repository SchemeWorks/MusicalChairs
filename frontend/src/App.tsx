import React, { useState, useRef } from 'react';
import { useInternetIdentity } from './hooks/useInternetIdentity';
import { useGetCallerUserProfile, useGetInternalWalletBalance } from './hooks/useQueries';
import LoginButton from './components/LoginButton';
import ProfileSetup from './components/ProfileSetup';
import Dashboard from './components/Dashboard';
import LoadingSpinner from './components/LoadingSpinner';
import ConfettiCanvas from './components/ConfettiCanvas';
import WalletDropdown from './components/WalletDropdown';
import LogoutButton from './components/LogoutButton';
import ErrorBoundary from './components/ErrorBoundary';
import ShenanigansAdminPanel from './components/ShenanigansAdminPanel';
import { Toaster } from '@/components/ui/sonner';
import { Button } from '@/components/ui/button';
import { Menu } from 'lucide-react';

const ADMIN_PRINCIPAL = 's4pq6-pomas-5qmdu-jw7n4-woskx-ijcqr-yph6i-uqi4k-5kaog-vv6us-qqe';

export default function App() {
  const { identity, isInitializing } = useInternetIdentity();
  const { data: userProfile, isLoading: profileLoading, isFetched } = useGetCallerUserProfile();
  const { data: balanceData } = useGetInternalWalletBalance();
  const [isWalletDropdownOpen, setIsWalletDropdownOpen] = useState(false);
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);
  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const walletButtonRef = useRef<HTMLButtonElement>(null);

  const isAuthenticated = !!identity;
  const showProfileSetup = isAuthenticated && !profileLoading && isFetched && userProfile === null;
  const internalBalance = balanceData?.internalBalance || 0;
  const houseRepaymentBalance = balanceData?.houseRepaymentBalance || 0;
  const showDashboard = isAuthenticated && !showProfileSetup && !profileLoading;
  
  // Check if current user is admin
  const isAdmin = isAuthenticated && identity?.getPrincipal().toString() === ADMIN_PRINCIPAL;

  if (isInitializing) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-purple-600 via-pink-500 to-orange-400 flex items-center justify-center">
        <LoadingSpinner />
      </div>
    );
  }

  return (
    <ErrorBoundary>
      <div className="min-h-screen bg-gradient-to-br from-purple-600 via-pink-500 to-orange-400 relative flex flex-col">
        <ConfettiCanvas />
        
        {/* Fixed Header with playful design */}
        <header className="fixed top-0 left-0 right-0 z-40 bg-dark-navy border-b border-gold shadow-lg">
          <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
            <div className="flex items-center h-20">
              {/* Mobile Hamburger Menu - Far Left */}
              {showDashboard && !showAdminPanel && (
                <div className="md:hidden mr-4">
                  <Button
                    onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
                    className="mobile-menu-button bg-black/80 hover:bg-black/90 border-2 border-purple-500 p-3 rounded-lg shadow-lg backdrop-blur-sm"
                    variant="ghost"
                    size="icon"
                  >
                    <Menu className="h-6 w-6 text-white" />
                  </Button>
                </div>
              )}

              {/* Left side - Site title and tagline with responsive logo */}
              <div className="flex flex-col">
                <div className="flex items-center">
                  {/* Desktop: Full "Musical Chairs" logo */}
                  <span className="hidden md:block text-2xl font-black musical-chairs-title">
                    Musical Chairs
                  </span>
                  {/* Mobile: "MC" logo */}
                  <span className="block md:hidden text-2xl font-black musical-chairs-title">
                    MC
                  </span>
                </div>
                <div className="ponzi-tagline text-yellow-300 text-lg">
                  It's a Ponzi!
                </div>
              </div>
              
              {/* Center area - intentionally left blank */}
              <div className="flex-1"></div>
              
              {/* Right side - Admin, Wallet and Logout buttons */}
              <div className="flex items-center space-x-4">
                {isAuthenticated ? (
                  <>
                    {/* Admin Button - Only visible to admin principal */}
                    {isAdmin && (
                      <button
                        onClick={() => setShowAdminPanel(!showAdminPanel)}
                        className={`
                          flex items-center space-x-2 px-4 py-2 rounded-lg font-medium transition-all duration-200 shadow-lg hover:shadow-xl
                          ${showAdminPanel 
                            ? 'bg-yellow-600 hover:bg-yellow-700 text-white' 
                            : 'bg-yellow-400 hover:bg-yellow-500 text-black'
                          }
                        `}
                      >
                        <span>Admin</span>
                      </button>
                    )}
                    
                    {/* Wallet Button - Redesigned */}
                    <button
                      ref={walletButtonRef}
                      onClick={() => setIsWalletDropdownOpen(!isWalletDropdownOpen)}
                      className="wallet-button-new flex items-center space-x-2 bg-gradient-to-r from-green-500 to-blue-500 hover:from-green-600 hover:to-blue-600 text-white px-4 py-2 rounded-lg font-medium transition-all duration-200 shadow-lg hover:shadow-xl"
                    >
                      <span className="text-lg">💳</span>
                      <span>Wallet</span>
                    </button>
                    
                    {/* Logout Button */}
                    <LogoutButton />
                  </>
                ) : (
                  <LoginButton />
                )}
              </div>
            </div>
          </div>
        </header>

        {/* Main Content with top padding to account for fixed header */}
        <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 pt-28 grow">
          <ErrorBoundary fallback={
            <div className="text-center py-16">
              <div className="text-6xl mb-4">🎰</div>
              <h2 className="text-2xl font-bold text-white mb-4">Content Loading Error</h2>
              <p className="text-white/80 mb-6">
                There was an issue loading the main content. Please refresh the page.
              </p>
              <button
                onClick={() => window.location.reload()}
                className="bg-gradient-to-r from-green-500 to-blue-500 hover:from-green-600 hover:to-blue-600 text-white px-6 py-3 rounded-lg font-bold transition-all duration-200"
              >
                🔄 Refresh Page
              </button>
            </div>
          }>
            {!isAuthenticated ? (
              <div className="text-center py-16">
                <div className="max-w-2xl mx-auto">
                  {/* Welcome Header with backdrop blur */}
                  <div className="relative inline-block mb-2">
                    <h2 className="text-4xl font-black musical-chairs-title-with-backdrop">
                      Welcome to Musical Chairs!
                    </h2>
                  </div>
                  <div className="mb-6"></div>
                  <div className="ponzi-tagline text-yellow-300 text-3xl font-bold mb-8">
                    It's a Ponzi!
                  </div>

                  {/* Single Animated Gradient Frosted-Glass Outer Card */}
                  <div className="login-outer-card">
                    {/* Slot Machine Icon - Top Center */}
                    <div className="text-6xl mb-6 slot-icon">🎰</div>
                    
                    {/* Green Earnings Info Box - Separate Card */}
                    <div className="login-inner-green-card mb-6">
                      <div className="flex items-start space-x-3">
                        <div className="text-2xl">🚀</div>
                        <div className="text-left">
                          <p className="text-green-800 font-bold text-lg leading-relaxed">
                            Earn up to 12% daily! 💸<br />
                            Earnings accumulate in real time! 📈<br />
                            Withdraw earnings at any time, or lock your deposit to compound and have a chance at a face-melting ROI! 🤯
                          </p>
                        </div>
                      </div>
                    </div>

                    {/* Red Gambling Warning Box - Separate Card */}
                    <div className="login-inner-red-card mb-6">
                      <div className="flex items-start space-x-3">
                        <div className="text-2xl">⚠️</div>
                        <div className="text-left">
                          <p className="text-red-800 font-bold text-xl">
                            THIS IS A GAMBLING GAME!
                          </p>
                          <p className="text-red-700 font-semibold">
                            Only play with money you can afford to lose!
                          </p>
                        </div>
                      </div>
                    </div>

                    {/* Orange Game Reset Warning Box - Separate Card */}
                    <div className="login-inner-orange-card mb-6">
                      <div className="flex items-start space-x-3">
                        <div className="text-2xl">🎲</div>
                        <div className="text-left">
                          <p className="text-orange-800 font-bold">
                            When the pot runs empty, the game resets and all pending payouts are voided!
                          </p>
                        </div>
                      </div>
                    </div>
                  </div>
                  
                  {/* Internet Computer Logo with Subtle Dark Halo */}
                  <div className="flex justify-center mt-8">
                    <div className="ic-logo-container-dark">
                      <img 
                        src="https://internetcomputer.org/img/IC_logo_horizontal_white.svg" 
                        alt="Internet Computer" 
                        className="h-12 w-auto relative z-10 px-4 py-2"
                      />
                    </div>
                  </div>
                </div>
              </div>
            ) : showProfileSetup ? (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <div className="text-6xl mb-4">🎭</div>
                  <h2 className="text-2xl font-bold text-white mb-4">Profile Setup Error</h2>
                  <p className="text-white/80 mb-6">
                    There was an issue with the profile setup. Please try logging out and back in.
                  </p>
                </div>
              }>
                <ProfileSetup />
              </ErrorBoundary>
            ) : profileLoading ? (
              <div className="flex justify-center py-16">
                <LoadingSpinner />
              </div>
            ) : showAdminPanel ? (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <div className="text-6xl mb-4">🔧</div>
                  <h2 className="text-2xl font-bold text-white mb-4">Admin Panel Error</h2>
                  <p className="text-white/80 mb-6">
                    There was an issue loading the admin panel. Please refresh the page.
                  </p>
                  <button
                    onClick={() => window.location.reload()}
                    className="bg-gradient-to-r from-green-500 to-blue-500 hover:from-green-600 hover:to-blue-600 text-white px-6 py-3 rounded-lg font-bold transition-all duration-200"
                  >
                    🔄 Refresh Page
                  </button>
                </div>
              }>
                <ShenanigansAdminPanel />
              </ErrorBoundary>
            ) : (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <div className="text-6xl mb-4">🎪</div>
                  <h2 className="text-2xl font-bold text-white mb-4">Dashboard Error</h2>
                  <p className="text-white/80 mb-6">
                    There was an issue loading the dashboard. Please refresh the page.
                  </p>
                  <button
                    onClick={() => window.location.reload()}
                    className="bg-gradient-to-r from-green-500 to-blue-500 hover:from-green-600 hover:to-blue-600 text-white px-6 py-3 rounded-lg font-bold transition-all duration-200"
                  >
                    🔄 Refresh Dashboard
                  </button>
                </div>
              }>
                <Dashboard 
                  isMobileMenuOpen={isMobileMenuOpen} 
                  setIsMobileMenuOpen={setIsMobileMenuOpen} 
                />
              </ErrorBoundary>
            )}
          </ErrorBoundary>
        </main>

        {/* Wallet Dropdown */}
        <ErrorBoundary fallback={
          <div className="fixed top-20 right-4 bg-red-500 text-white p-4 rounded-lg z-50">
            Wallet Error - Please refresh
          </div>
        }>
          <WalletDropdown 
            isOpen={isWalletDropdownOpen} 
            onClose={() => setIsWalletDropdownOpen(false)}
            buttonRef={walletButtonRef}
          />
        </ErrorBoundary>

        {/* Toast Notifications */}
        <Toaster 
          position="top-center"
          toastOptions={{
            style: {
              background: 'linear-gradient(135deg, #10b981, #3b82f6)',
              color: 'white',
              border: '2px solid #fbbf24',
              fontSize: '16px',
              fontWeight: 'bold',
              padding: '16px 20px',
              borderRadius: '12px',
              boxShadow: '0 10px 25px rgba(0, 0, 0, 0.2)',
            },
            duration: 4000,
          }}
        />
      </div>
    </ErrorBoundary>
  );
}
