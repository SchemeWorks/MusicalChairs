import React, { useState } from 'react';
import { useWallet, WalletType } from '../hooks/useWallet';
import { X, Wallet, ExternalLink, Loader2, Check, AlertCircle } from 'lucide-react';

interface WalletOption {
  type: WalletType;
  name: string;
  description: string;
  iconUrl: string;
  iconEmoji?: string; // Fallback emoji if image fails
  installed?: boolean;
  installUrl?: string;
}

interface WalletConnectModalProps {
  isOpen: boolean;
  onClose: () => void;
}

// Wallet logos as inline SVG data URIs - Official logos
const ICP_LOGO = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 360"><defs><linearGradient id="bg" x1="17.42" x2="340.066" y1="64.563" y2="-59.596" gradientTransform="matrix(1 0 0 -1 0 182)" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#292a2e"/><stop offset="1" stop-color="#555"/></linearGradient><linearGradient id="g1" x1="149.498" x2="72.885" y1="228.805" y2="149.47" gradientUnits="userSpaceOnUse"><stop offset=".22" stop-color="#ed1e79"/><stop offset=".892" stop-color="#522785"/></linearGradient><linearGradient id="g2" x1="-313.26" x2="-389.873" y1="-296.506" y2="-375.841" gradientTransform="rotate(180 -51.404 -81.632)" gradientUnits="userSpaceOnUse"><stop offset=".21" stop-color="#f15a24"/><stop offset=".684" stop-color="#fbb03b"/></linearGradient></defs><circle cx="180" cy="180" r="180" fill="url(#bg)"/><path fill="#29abe2" d="M254.8 217.4c-15.9 0-32.7-10.4-40.9-17.9-9-8.3-33.8-35.1-33.9-35.2-16.2-18.1-38.1-38.2-59.9-38.2-26.2 0-49.1 18.2-55.1 42.2.5-1.6 8.8-23.6 40.1-23.6 15.9 0 32.7 10.4 40.9 17.9 9 8.3 33.8 35.1 33.9 35.2 16.2 18.1 38.1 38.2 59.9 38.2 26.2 0 49.1-18.2 55.1-42.2-.4 1.5-8.8 23.6-40.1 23.6z"/><path fill="url(#g1)" d="M180 197.8c-.1-.1-7.2-7.8-15.2-16.2-4.3 5.1-10.5 12.1-17.7 18.4-13.3 11.7-22 14.1-27 14.1-18.7 0-34-14.8-34-33.1 0-18.1 15.2-33 34-33.1.7 0 1.5.1 2.5.2-5.6-2.2-11.6-3.6-17.5-3.6-31.3 0-39.6 22-40.1 23.6-1 4.1-1.6 8.4-1.6 12.8 0 30.3 25 55 56.2 55 13 0 27.6-6.7 42.6-19.8 7.1-6.2 13.2-12.9 17.9-18.2 0 0 0-.1-.1-.1z"/><path fill="url(#g2)" d="M180 164.2c.1.1 7.2 7.8 15.2 16.2 4.3-5.1 10.5-12.1 17.7-18.4 13.3-11.7 22-14.1 27-14.1 18.7 0 34 14.8 34 33.1 0 18.1-15.2 33-34 33.1-.7 0-1.5-.1-2.5-.2 5.6 2.2 11.6 3.6 17.5 3.6 31.3 0 39.6-22 40.1-23.6 1-4.1 1.6-8.4 1.6-12.8 0-30.3-25.4-55-56.6-55-13 0-27.2 6.7-42.2 19.8-7.1 6.2-13.2 12.9-17.9 18.2 0 0 0 .1.1.1z"/></svg>`;

const PLUG_LOGO = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2701 3925"><defs><linearGradient id="p0" x1="1801.69" y1="1816.06" x2="2773.04" y2="843.718" gradientUnits="userSpaceOnUse"><stop stop-color="#46FF47"/><stop offset="1" stop-color="#9CFF9D"/></linearGradient><linearGradient id="p1" x1="1566.99" y1="1795.81" x2="1896.78" y2="1223.9" gradientUnits="userSpaceOnUse"><stop stop-color="#10D9ED"/><stop offset="1" stop-color="#10D9ED" stop-opacity="0.3"/></linearGradient><linearGradient id="p2" x1="1460.48" y1="2039.52" x2="1649.76" y2="1535.91" gradientUnits="userSpaceOnUse"><stop stop-color="#FA51D3"/><stop offset="0.958774" stop-color="#FA51D3" stop-opacity="0"/></linearGradient><linearGradient id="p3" x1="1002.5" y1="2823" x2="1306.54" y2="2013.26" gradientUnits="userSpaceOnUse"><stop stop-color="#FFE700"/><stop offset="1" stop-color="#FFE700" stop-opacity="0"/></linearGradient><linearGradient id="p4" x1="1351.01" y1="2337.16" x2="1351.01" y2="2922.93" gradientUnits="userSpaceOnUse"><stop/><stop offset="1" stop-opacity="0.65"/></linearGradient><linearGradient id="p5" x1="1540.35" y1="2421.97" x2="1540.35" y2="2579.76" gradientUnits="userSpaceOnUse"><stop offset="0.75" stop-color="white"/><stop offset="1" stop-color="#DEDEDF"/></linearGradient><linearGradient id="p6" x1="1020.38" y1="1469.78" x2="1020.37" y2="1993.67" gradientUnits="userSpaceOnUse"><stop offset="0.75" stop-color="white"/><stop offset="1" stop-color="#DEDEDF"/></linearGradient><linearGradient id="p7" x1="1677.62" y1="1469.78" x2="1677.63" y2="1993.67" gradientUnits="userSpaceOnUse"><stop offset="0.75" stop-color="white"/><stop offset="1" stop-color="#DEDEDF"/></linearGradient><linearGradient id="p8" x1="1350" y1="1272" x2="1350" y2="2197" gradientUnits="userSpaceOnUse"><stop/><stop offset="1" stop-opacity="0.65"/></linearGradient></defs><path d="M0 1123.78C0 1036.64 70.642 966 157.783 966H2542.28C2629.43 966 2700.07 1036.64 2700.07 1123.78V1909.74C2700.07 2655.34 2095.64 3259.78 1350.03 3259.78C604.431 3259.78 0 2655.34 0 1909.74V1123.78Z" fill="url(#p0)"/><path fill-rule="evenodd" clip-rule="evenodd" d="M2593.17 2437.3C2387.77 2920.83 1908.5 3259.94 1350.03 3259.94C604.431 3259.94 0 2655.51 0 1909.9V1342.16C256.416 1200.58 551.223 1120 864.85 1120C1690.17 1120 2385.17 1677.99 2593.17 2437.3Z" fill="url(#p1)"/><path fill-rule="evenodd" clip-rule="evenodd" d="M2270.38 2898.52C2029.24 3123.3 1705.7 3260.84 1350.03 3260.84C604.431 3260.84 0 2656.41 0 1910.8V1766.48C228.495 1623.59 498.568 1541 787.931 1541C1565.79 1541 2204.25 2137.82 2270.38 2898.52Z" fill="url(#p2)"/><path fill-rule="evenodd" clip-rule="evenodd" d="M1804.3 3188.71C1776.22 2565.56 1264 2069 636.222 2069C418.032 2069 213.799 2128.98 39 2233.42C147.688 2678.73 475.344 3037.76 900.894 3188.98V3264.44C900.894 3373.69 959.59 3469.18 1047.06 3520.88V3726.37C1047.06 3835.86 1135.49 3924.61 1244.57 3924.61H1459.86C1568.95 3924.61 1657.38 3835.86 1657.38 3726.37V3520.88C1744.85 3469.18 1803.54 3373.69 1803.54 3264.44V3188.98C1803.8 3188.89 1804.05 3188.8 1804.3 3188.71Z" fill="url(#p3)"/><path d="M504 60C504 26.8629 530.863 0 564 0H838C871.137 0 898 26.8629 898 60V966H504V60Z" fill="#031514"/><path d="M1792 60C1792 26.8629 1818.86 0 1852 0H2126C2159.14 0 2186 26.8629 2186 60V966H1792V60Z" fill="#031514"/><path d="M672.133 2339.6C665.796 2338.36 659.96 2343.39 660.365 2349.84C680.428 2669.31 981.987 2922.93 1351.05 2922.93C1719.27 2922.93 2020.29 2670.48 2041.6 2352.05C2042.03 2345.6 2036.21 2340.54 2029.87 2341.76C1815.09 2383.12 1584 2423.42 1351.05 2423.42C1114.02 2423.42 889.87 2382.34 672.133 2339.6Z" fill="url(#p4)"/><path d="M703.109 2390.42C714.991 2490.79 1007.42 2613.29 1355.49 2613.29C1702.77 2613.29 1989.78 2490.48 2002.85 2390.42C1799.68 2403.43 1782.94 2468.14 1355.49 2468.14C906.255 2468.14 909.406 2403.86 703.109 2390.42Z" fill="url(#p5)"/><path d="M1299 1734.95C1197.43 1921.41 1038.66 2109.84 855.234 2104.91C667.88 2104.91 516 1939.27 516 1734.95C516 1530.63 667.88 1365 855.234 1365C1042.59 1365 1180.66 1528.77 1299 1734.95Z" fill="url(#p6)"/><path d="M1399 1735.49C1500.57 1918.24 1659.34 2109.75 1842.77 2104.91C2030.12 2104.91 2182 1935.75 2182 1735.49C2182 1535.23 2031.11 1365 1843.75 1365C1656.4 1365 1517.34 1533.4 1399 1735.49Z" fill="url(#p7)"/><circle cx="987.542" cy="1797.18" r="89.1743" fill="#031514"/><circle cx="1018.7" cy="1836.94" r="30.0829" fill="white"/><circle cx="1711.37" cy="1797.18" r="89.1743" fill="#031514"/><circle cx="1742.53" cy="1836.94" r="30.0829" fill="white"/><path d="M1300.87 1738.58L517.972 1779C516.283 1764.21 515.021 1757.31 515.019 1746.47C513.047 1528.61 665.883 1363 858.155 1363C1050.43 1363 1200.29 1553.26 1298.9 1733.65C1298.07 1735.17 1301.71 1737.06 1300.87 1738.58Z" fill="#031514"/><path d="M1397.13 1738.58L2180.83 1779C2182.52 1764.21 2182.78 1757.31 2182.78 1746.47C2189.71 1532.55 2031.95 1363 1839.73 1363C1647.51 1363 1497.68 1553.26 1399.1 1733.65C1399.93 1735.17 1396.29 1737.06 1397.13 1738.58Z" fill="#031514"/><path d="M1838.17 1272C1599.49 1272 1425.76 1510.33 1350 1637.52C1274.24 1510.33 1100.51 1272 861.831 1272C620.927 1272 425 1479.49 425 1734.5C425 1989.51 620.927 2197 861.831 2197C1100.48 2197 1274.24 1958.67 1350 1831.45C1425.76 1958.67 1599.49 2197 1838.17 2197C2079.07 2197 2275 1989.51 2275 1734.5C2275 1479.49 2079.07 1272 1838.17 1272ZM861.831 2104.5C671.975 2104.5 517.5 1938.52 517.5 1734.5C517.5 1530.48 671.975 1364.5 861.831 1364.5C1087.39 1364.5 1256.31 1655.33 1298.14 1734.5C1256.31 1813.67 1087.42 2104.5 861.831 2104.5ZM1838.17 2104.5C1612.61 2104.5 1443.69 1813.67 1401.86 1734.5C1443.69 1655.33 1612.61 1364.5 1838.17 1364.5C2028.03 1364.5 2182.5 1530.48 2182.5 1734.5C2182.5 1938.52 2028.03 2104.5 1838.17 2104.5Z" fill="url(#p8)"/></svg>`;

const OISY_LOGO = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="o0" x1="32" y1="0" x2="32" y2="64" gradientUnits="userSpaceOnUse"><stop stop-color="#0066FF"/><stop offset="1" stop-color="#0066FF"/></linearGradient><clipPath id="oc"><rect width="64" height="64" fill="white"/></clipPath></defs><g clip-path="url(#oc)"><circle cx="32" cy="32" r="32" fill="url(#o0)"/><path fill-rule="evenodd" clip-rule="evenodd" d="M35.5086 48.6098L34.7387 52.0151C34.6237 52.5235 34.1374 52.859 33.6222 52.7793C32.4482 52.5979 31.2584 52.3894 30.0564 52.1591C29.5045 52.0534 29.1526 51.5103 29.2765 50.9622L29.9735 47.8793C29.4854 47.7837 28.9942 47.6803 28.5005 47.5697C28.0554 47.4682 27.6147 47.3622 27.1789 47.2517L26.4822 50.3336C26.3583 50.8815 25.8076 51.2203 25.264 51.079C24.0798 50.771 22.916 50.449 21.778 50.1097C21.2778 49.9605 20.9822 49.448 21.0973 48.9389L21.8632 45.5512C14.2939 42.5159 9.51524 37.3571 11.5435 28.5473L12.0315 26.3886C13.9943 17.5457 20.5251 14.9543 28.6628 15.4753L29.4282 12.0899C29.5433 11.5807 30.0307 11.2452 30.5465 11.3258C31.7197 11.5091 32.9088 11.719 34.1103 11.9505C34.6618 12.0567 35.0132 12.5995 34.8893 13.1473L34.1926 16.2292C34.6335 16.3169 35.0769 16.4109 35.5224 16.5107C36.016 16.6223 36.5041 16.7393 36.986 16.8621L37.6823 13.782C37.8062 13.2339 38.3575 12.895 38.9012 13.037C40.0853 13.3461 41.2491 13.6697 42.387 14.0108C42.8864 14.1605 43.1812 14.6726 43.0662 15.1811L42.2985 18.577C49.7691 21.6116 54.4382 26.7666 52.4573 35.5281L51.9693 37.6869C50.0233 46.4547 43.5626 49.0795 35.5086 48.6098ZM46.4916 35.933L46.7663 34.7182C48.7004 26.6813 42.2694 23.6796 34.3411 21.771C26.3257 20.075 19.2288 20.0183 17.5176 28.1056L17.2429 29.3204C15.3004 37.3941 21.7315 40.3958 29.6969 42.3128C37.6752 44.0004 44.7721 44.0571 46.4916 35.933Z" fill="white"/></g></svg>`;

const WALLET_LOGOS = {
  'internet-identity': `data:image/svg+xml,${encodeURIComponent(ICP_LOGO)}`,
  'plug': `data:image/svg+xml,${encodeURIComponent(PLUG_LOGO)}`,
  'oisy': `data:image/svg+xml,${encodeURIComponent(OISY_LOGO)}`,
};

export default function WalletConnectModal({ isOpen, onClose }: WalletConnectModalProps) {
  const { connect, isConnecting } = useWallet();
  const [selectedWallet, setSelectedWallet] = useState<WalletType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [imageErrors, setImageErrors] = useState<Record<string, boolean>>({});

  // Check if Plug is installed
  const isPlugInstalled = typeof window !== 'undefined' && !!window.ic?.plug;

  const walletOptions: WalletOption[] = [
    {
      type: 'internet-identity',
      name: 'Internet Identity',
      description: 'Secure authentication using ICP\'s native identity system',
      iconUrl: WALLET_LOGOS['internet-identity'],
      iconEmoji: '🔐',
      installed: true,
    },
    {
      type: 'plug',
      name: 'Plug Wallet',
      description: 'Popular ICP browser wallet extension',
      iconUrl: WALLET_LOGOS['plug'],
      iconEmoji: '🔌',
      installed: isPlugInstalled,
      installUrl: 'https://plugwallet.ooo/',
    },
    {
      type: 'oisy',
      name: 'OISY Wallet',
      description: 'Multi-chain wallet with Internet Identity',
      iconUrl: WALLET_LOGOS['oisy'],
      iconEmoji: '✨',
      installed: true, // OISY uses II, always available
    },
  ];

  const handleConnect = async (walletType: WalletType) => {
    setError(null);
    setSelectedWallet(walletType);

    try {
      await connect(walletType);
      onClose();
    } catch (err: any) {
      console.error('Connection error:', err);
      setError(err.message || 'Failed to connect wallet');
      setSelectedWallet(null);
    }
  };

  const handleImageError = (walletType: string) => {
    setImageErrors(prev => ({ ...prev, [walletType]: true }));
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div 
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      
      {/* Modal */}
      <div className="relative bg-gradient-to-br from-purple-900/95 via-purple-800/95 to-indigo-900/95 rounded-2xl border border-purple-500/30 shadow-2xl w-full max-w-md mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-purple-500/20">
          <div className="flex items-center gap-3">
            <Wallet className="w-6 h-6 text-purple-300" />
            <h2 className="text-xl font-bold text-white">Connect Wallet</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 rounded-lg hover:bg-purple-700/50 transition-colors"
          >
            <X className="w-5 h-5 text-purple-300" />
          </button>
        </div>

        {/* Wallet Options */}
        <div className="p-6 space-y-3">
          {walletOptions.map((wallet) => (
            <WalletOptionButton
              key={wallet.type}
              wallet={wallet}
              isConnecting={isConnecting && selectedWallet === wallet.type}
              isSelected={selectedWallet === wallet.type}
              onClick={() => handleConnect(wallet.type)}
              imageError={imageErrors[wallet.type]}
              onImageError={() => handleImageError(wallet.type)}
            />
          ))}
        </div>

        {/* Error Message */}
        {error && (
          <div className="mx-6 mb-6 p-4 bg-red-500/20 border border-red-500/30 rounded-lg flex items-start gap-3">
            <AlertCircle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
            <p className="text-red-200 text-sm">{error}</p>
          </div>
        )}

        {/* Footer */}
        <div className="px-6 pb-6">
          <p className="text-purple-300/60 text-xs text-center">
            By connecting, you agree to our Terms of Service and acknowledge that this is a gambling game.
          </p>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Wallet Option Button Component
// ============================================================================

interface WalletOptionButtonProps {
  wallet: WalletOption;
  isConnecting: boolean;
  isSelected: boolean;
  onClick: () => void;
  imageError: boolean;
  onImageError: () => void;
}

function WalletOptionButton({ wallet, isConnecting, isSelected, onClick, imageError, onImageError }: WalletOptionButtonProps) {
  const isDisabled = isConnecting || (!wallet.installed && wallet.type === 'plug');

  // Render wallet icon (image or emoji fallback)
  const renderIcon = () => {
    if (imageError || !wallet.iconUrl) {
      return <span className="text-3xl">{wallet.iconEmoji}</span>;
    }
    return (
      <img 
        src={wallet.iconUrl} 
        alt={`${wallet.name} logo`}
        className="w-10 h-10 object-contain"
        onError={onImageError}
      />
    );
  };

  if (!wallet.installed && wallet.installUrl) {
    // Show install prompt for uninstalled wallets
    return (
      <a
        href={wallet.installUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="flex items-center gap-4 p-4 rounded-xl bg-purple-800/30 border border-purple-500/20 hover:border-purple-400/40 transition-all group"
      >
        <div className="w-10 h-10 flex items-center justify-center">
          {renderIcon()}
        </div>
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <span className="font-semibold text-white">{wallet.name}</span>
            <span className="text-xs px-2 py-0.5 bg-yellow-500/20 text-yellow-300 rounded-full">
              Not Installed
            </span>
          </div>
          <p className="text-purple-300/70 text-sm mt-0.5">{wallet.description}</p>
        </div>
        <ExternalLink className="w-5 h-5 text-purple-400 group-hover:text-purple-300 transition-colors" />
      </a>
    );
  }

  return (
    <button
      onClick={onClick}
      disabled={isDisabled}
      className={`
        flex items-center gap-4 p-4 rounded-xl w-full text-left transition-all
        ${isSelected 
          ? 'bg-purple-600/50 border-purple-400/60 shadow-lg shadow-purple-500/20' 
          : 'bg-purple-800/30 hover:bg-purple-700/40 border-purple-500/20 hover:border-purple-400/40'
        }
        border disabled:opacity-50 disabled:cursor-not-allowed
      `}
    >
      <div className="w-10 h-10 flex items-center justify-center">
        {renderIcon()}
      </div>
      <div className="flex-1">
        <span className="font-semibold text-white">{wallet.name}</span>
        <p className="text-purple-300/70 text-sm mt-0.5">{wallet.description}</p>
      </div>
      {isConnecting ? (
        <Loader2 className="w-5 h-5 text-purple-300 animate-spin" />
      ) : isSelected ? (
        <Check className="w-5 h-5 text-green-400" />
      ) : null}
    </button>
  );
}
