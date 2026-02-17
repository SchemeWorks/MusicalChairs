import React, { useState } from 'react';

const charlesLoadingLines = [
  'Charles is counting the money...',
  'Charles is shuffling the deck...',
  'Warming up the Ponzi engine...',
  'Charles is reviewing your application...',
  'Checking if the pot is still there...',
];

export default function LoadingSpinner() {
  const [line] = useState(() => charlesLoadingLines[Math.floor(Math.random() * charlesLoadingLines.length)]);

  return (
    <div className="flex flex-col items-center justify-center p-8 gap-3">
      <div className="mc-spinner" />
      <p className="text-xs mc-text-muted font-accent italic animate-pulse">{line}</p>
    </div>
  );
}
