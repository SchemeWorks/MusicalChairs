import React, { useState, useEffect, useRef } from 'react';
import { Dialog, DialogContent } from '@/components/ui/dialog';

interface MobileSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}

export default function MobileSheet({ open, onOpenChange, children }: MobileSheetProps) {
  const [isMobile, setIsMobile] = useState(false);
  const sheetRef = useRef<HTMLDivElement>(null);
  const startY = useRef(0);

  useEffect(() => {
    const check = () => setIsMobile(window.innerWidth < 769);
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, []);

  const handleTouchStart = (e: React.TouchEvent) => {
    startY.current = e.touches[0].clientY;
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!sheetRef.current) return;
    const deltaY = e.touches[0].clientY - startY.current;
    if (deltaY > 0) {
      sheetRef.current.style.transform = `translateY(${deltaY}px)`;
    }
  };

  const handleTouchEnd = (e: React.TouchEvent) => {
    if (!sheetRef.current) return;
    const deltaY = e.changedTouches[0].clientY - startY.current;
    if (deltaY > sheetRef.current.offsetHeight * 0.3) {
      onOpenChange(false);
    }
    sheetRef.current.style.transform = '';
  };

  if (isMobile) {
    if (!open) return null;
    return (
      <>
        <div className="mc-sheet-backdrop" onClick={() => onOpenChange(false)} />
        <div ref={sheetRef} className="mc-bottom-sheet p-6">
          <div
            className="w-10 h-1 rounded-full bg-white/20 mx-auto mb-4 cursor-grab"
            onTouchStart={handleTouchStart}
            onTouchMove={handleTouchMove}
            onTouchEnd={handleTouchEnd}
          />
          {children}
        </div>
      </>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="mc-dialog">
        {children}
      </DialogContent>
    </Dialog>
  );
}
