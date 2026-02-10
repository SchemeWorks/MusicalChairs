import React, { useEffect, useState } from 'react';
import { formatICP } from '../lib/formatICP';

interface HouseMoneyToastProps {
  amount: number;
  ponziPoints: number;
  onClose: () => void;
}

export default function HouseMoneyToast({ amount, ponziPoints, onClose }: HouseMoneyToastProps) {
  const [isVisible, setIsVisible] = useState(false);
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    // Fade in animation
    setTimeout(() => setIsVisible(true), 10);

    // Trigger confetti particles
    createConfettiParticles();

    // Auto-dismiss after 3 seconds
    const dismissTimer = setTimeout(() => {
      handleClose();
    }, 3000);

    return () => clearTimeout(dismissTimer);
  }, []);

  const handleClose = () => {
    setIsExiting(true);
    setTimeout(() => {
      onClose();
    }, 1000);
  };

  const createConfettiParticles = () => {
    const container = document.getElementById('house-toast-confetti');
    if (!container) return;

    // Create 3-4 gentle sparkle particles
    for (let i = 0; i < 4; i++) {
      const particle = document.createElement('div');
      particle.className = 'house-toast-particle';
      
      const size = Math.random() * 6 + 4; // 4-10px
      const startX = Math.random() * 100 - 50; // -50 to 50
      const startY = Math.random() * 100 - 50;
      const endX = startX + (Math.random() * 100 - 50);
      const endY = startY + (Math.random() * 100 - 50);
      
      particle.style.width = `${size}px`;
      particle.style.height = `${size}px`;
      particle.style.left = `calc(50% + ${startX}px)`;
      particle.style.top = `calc(50% + ${startY}px)`;
      
      const colors = ['#FFD700', '#FF6B6B', '#4ECDC4', '#F7DC6F'];
      particle.style.backgroundColor = colors[Math.floor(Math.random() * colors.length)];
      
      const keyframes = [
        { transform: 'translate(0, 0) scale(1)', opacity: '1' },
        { transform: `translate(${endX}px, ${endY}px) scale(0)`, opacity: '0' }
      ];
      
      particle.animate(keyframes, {
        duration: 1000,
        easing: 'ease-out',
        fill: 'forwards'
      });
      
      container.appendChild(particle);
      
      setTimeout(() => {
        if (container.contains(particle)) {
          container.removeChild(particle);
        }
      }, 1000);
    }
  };

  return (
    <div
      className={`fixed top-8 left-1/2 transform -translate-x-1/2 z-[9999] transition-all duration-300 ease-out ${
        isVisible && !isExiting ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-4'
      }`}
      style={{ pointerEvents: 'auto' }}
    >
      {/* Confetti container */}
      <div id="house-toast-confetti" className="absolute inset-0 pointer-events-none" />
      
      {/* Toast card */}
      <div className="house-money-toast-card relative">
        {/* Content */}
        <div className="text-center space-y-3">
          {/* Title */}
          <div className="text-2xl font-black text-white">
            🎉 Deposit Successful!
          </div>
          
          {/* Subtitle with gradient accent on numbers */}
          <div className="text-base text-white/90 leading-relaxed">
            You've added{' '}
            <span className="house-toast-accent font-black">{formatICP(amount)} ICP</span>{' '}
            as house money and earned{' '}
            <span className="house-toast-accent font-black">{ponziPoints.toLocaleString()} Ponzi Points</span>!
          </div>
          
          {/* Fine print */}
          <div className="text-xs text-white/60 mt-2">
            The House thanks you for your generosity.
          </div>
        </div>
        
        {/* Nice button */}
        <button
          onClick={handleClose}
          className="house-toast-button mt-4 w-full"
        >
          Nice!
        </button>
      </div>
    </div>
  );
}

