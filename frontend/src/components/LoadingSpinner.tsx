import React from 'react';

export default function LoadingSpinner() {
  return (
    <div className="flex items-center justify-center">
      <div className="relative">
        <div className="animate-spin rounded-full h-12 w-12 border-4 border-white border-t-transparent"></div>
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="text-white text-xl">🎰</div>
        </div>
      </div>
    </div>
  );
}
