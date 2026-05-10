import React from 'react';
import BankSummary from './BankSummary';
import BridgeCard from './BridgeCard';
import PendingQueueCard from './PendingQueueCard';

export default function BankPage() {
  return (
    <div className="space-y-6 p-4">
      <BankSummary />
      <BridgeCard />
      <PendingQueueCard />
    </div>
  );
}
