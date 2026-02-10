import React, { useState } from 'react';
import { useWallet } from '../hooks/useWallet';
import { useQueryClient } from '@tanstack/react-query';

export default function LogoutButton() {
  const { disconnect, isConnecting } = useWallet();
  const queryClient = useQueryClient();
  const [isBlinking, setIsBlinking] = useState(false);

  const disabled = isConnecting;

  const handleLogout = async () => {
    setIsBlinking(true);
    
    // Blink effect (fade out/in once)
    setTimeout(async () => {
      await disconnect();
      queryClient.clear();
      setIsBlinking(false);
    }, 300);
  };

  return (
    <button
      onClick={handleLogout}
      disabled={disabled}
      className={`logout-button w-10 h-10 rounded-full bg-transparent border-none text-white text-xl transition-all duration-200 hover:shadow-red-glow hover:animate-pulse disabled:opacity-50 ${
        isBlinking ? 'animate-blink' : ''
      }`}
      title="Logout"
    >
      ⏻
    </button>
  );
}
