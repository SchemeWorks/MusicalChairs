import React from 'react';

export default function ConfettiCanvas() {
  return (
    <div className="fixed inset-0 pointer-events-none z-0">
      {/* Animated background elements */}
      <div className="absolute top-10 left-10 text-2xl animate-bounce opacity-20">🎰</div>
      <div className="absolute top-20 right-20 text-2xl animate-bounce opacity-20" style={{ animationDelay: '0.5s' }}>🎲</div>
      <div className="absolute bottom-20 left-20 text-2xl animate-bounce opacity-20" style={{ animationDelay: '1s' }}>🎪</div>
      <div className="absolute bottom-10 right-10 text-2xl animate-bounce opacity-20" style={{ animationDelay: '1.5s' }}>🎭</div>
      <div className="absolute top-1/2 left-1/4 text-2xl animate-bounce opacity-20" style={{ animationDelay: '2s' }}>💰</div>
      <div className="absolute top-1/3 right-1/3 text-2xl animate-bounce opacity-20" style={{ animationDelay: '2.5s' }}>🎯</div>
    </div>
  );
}

// Enhanced confetti trigger function with proper colorful paper pieces, streamers, and geometric shapes
export const triggerConfetti = () => {
  // Create temporary confetti container
  const confettiContainer = document.createElement('div');
  confettiContainer.className = 'fixed inset-0 pointer-events-none z-50 overflow-hidden';
  document.body.appendChild(confettiContainer);

  // Casino-themed colors for confetti pieces
  const colors = [
    '#FFD700', // Gold
    '#FF6B6B', // Red
    '#4ECDC4', // Teal
    '#45B7D1', // Blue
    '#96CEB4', // Green
    '#FFEAA7', // Yellow
    '#DDA0DD', // Plum
    '#FF7F50', // Coral
    '#98D8C8', // Mint
    '#F7DC6F', // Light Gold
    '#BB8FCE', // Light Purple
    '#85C1E9'  // Light Blue
  ];

  // Different confetti shapes
  const shapes = ['rectangle', 'circle', 'triangle', 'diamond'];

  // Create multiple confetti pieces
  for (let i = 0; i < 100; i++) {
    const confetti = document.createElement('div');
    const color = colors[Math.floor(Math.random() * colors.length)];
    const shape = shapes[Math.floor(Math.random() * shapes.length)];
    const size = Math.random() * 8 + 4; // 4-12px
    const startX = Math.random() * window.innerWidth;
    const startY = -20;
    const endY = window.innerHeight + 20;
    const rotation = Math.random() * 360;
    const duration = Math.random() * 3 + 2; // 2-5 seconds
    const delay = Math.random() * 0.5; // 0-0.5s delay
    const drift = (Math.random() - 0.5) * 200; // Horizontal drift

    // Set base styles
    confetti.style.position = 'absolute';
    confetti.style.left = startX + 'px';
    confetti.style.top = startY + 'px';
    confetti.style.width = size + 'px';
    confetti.style.height = size + 'px';
    confetti.style.backgroundColor = color;
    confetti.style.pointerEvents = 'none';
    confetti.style.zIndex = '9999';

    // Apply shape-specific styles
    switch (shape) {
      case 'rectangle':
        confetti.style.borderRadius = '2px';
        break;
      case 'circle':
        confetti.style.borderRadius = '50%';
        break;
      case 'triangle':
        confetti.style.width = '0';
        confetti.style.height = '0';
        confetti.style.backgroundColor = 'transparent';
        confetti.style.borderLeft = `${size/2}px solid transparent`;
        confetti.style.borderRight = `${size/2}px solid transparent`;
        confetti.style.borderBottom = `${size}px solid ${color}`;
        break;
      case 'diamond':
        confetti.style.transform = 'rotate(45deg)';
        confetti.style.borderRadius = '2px';
        break;
    }

    // Create keyframe animation
    const keyframes = [
      {
        transform: `translateY(0px) translateX(0px) rotate(${rotation}deg)`,
        opacity: '1'
      },
      {
        transform: `translateY(${endY}px) translateX(${drift}px) rotate(${rotation + 360}deg)`,
        opacity: '0'
      }
    ];

    const animation = confetti.animate(keyframes, {
      duration: duration * 1000,
      delay: delay * 1000,
      easing: 'cubic-bezier(0.25, 0.46, 0.45, 0.94)',
      fill: 'forwards'
    });

    confettiContainer.appendChild(confetti);

    // Clean up individual pieces when animation completes
    animation.addEventListener('finish', () => {
      if (confetti.parentNode) {
        confetti.parentNode.removeChild(confetti);
      }
    });
  }

  // Create some larger streamers for extra celebration effect
  for (let i = 0; i < 20; i++) {
    const streamer = document.createElement('div');
    const color = colors[Math.floor(Math.random() * colors.length)];
    const width = Math.random() * 4 + 2; // 2-6px width
    const height = Math.random() * 30 + 20; // 20-50px height
    const startX = Math.random() * window.innerWidth;
    const startY = -50;
    const endY = window.innerHeight + 50;
    const rotation = Math.random() * 360;
    const duration = Math.random() * 4 + 3; // 3-7 seconds
    const delay = Math.random() * 0.3;
    const drift = (Math.random() - 0.5) * 300;

    streamer.style.position = 'absolute';
    streamer.style.left = startX + 'px';
    streamer.style.top = startY + 'px';
    streamer.style.width = width + 'px';
    streamer.style.height = height + 'px';
    streamer.style.backgroundColor = color;
    streamer.style.borderRadius = width/2 + 'px';
    streamer.style.pointerEvents = 'none';
    streamer.style.zIndex = '9998';

    const keyframes = [
      {
        transform: `translateY(0px) translateX(0px) rotate(${rotation}deg)`,
        opacity: '0.9'
      },
      {
        transform: `translateY(${endY}px) translateX(${drift}px) rotate(${rotation + 180}deg)`,
        opacity: '0'
      }
    ];

    const animation = streamer.animate(keyframes, {
      duration: duration * 1000,
      delay: delay * 1000,
      easing: 'cubic-bezier(0.25, 0.46, 0.45, 0.94)',
      fill: 'forwards'
    });

    confettiContainer.appendChild(streamer);

    animation.addEventListener('finish', () => {
      if (streamer.parentNode) {
        streamer.parentNode.removeChild(streamer);
      }
    });
  }

  // Remove the entire container after all animations should be complete
  setTimeout(() => {
    if (document.body.contains(confettiContainer)) {
      document.body.removeChild(confettiContainer);
    }
  }, 8000);
};
