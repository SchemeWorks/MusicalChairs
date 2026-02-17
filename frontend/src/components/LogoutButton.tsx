import React, { useState } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useQueryClient } from '@tanstack/react-query';
import { Power } from 'lucide-react';

export default function LogoutButton() {
  const { disconnect, isConnecting } = useWallet();
  const queryClient = useQueryClient();
  const [isBlinking, setIsBlinking] = useState(false);

  const handleLogout = async () => {
    setIsBlinking(true);
    setTimeout(async () => {
      await disconnect();
      queryClient.clear();
      setIsBlinking(false);
    }, 300);
  };

  return (
    <button
      onClick={handleLogout}
      disabled={isConnecting}
      className={`w-10 h-10 rounded-lg flex items-center justify-center text-white/50 hover:text-red-400 hover:bg-red-500/10 transition-all duration-200 disabled:opacity-50 ${
        isBlinking ? 'opacity-30' : ''
      }`}
      title="Logout"
    >
      <Power className="h-4 w-4" />
    </button>
  );
}
