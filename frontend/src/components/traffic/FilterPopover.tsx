import React, { useState, useRef, useEffect } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';

/**
 * Toolbar button that toggles a dropdown panel and closes on outside click.
 * Used for the service / port / decision pickers in the traffic filter bar.
 */
export function FilterPopover({ label, icon, isActive, children }: { label: string, icon: React.ReactNode, isActive: boolean, children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handle = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    if (open) document.addEventListener('mousedown', handle);
    return () => document.removeEventListener('mousedown', handle);
  }, [open]);

  return (
    <div className="filter-popover" ref={ref}>
      <button
        className={`filter-popover__btn ${isActive ? 'filter-popover__btn--active' : ''}`}
        onClick={() => setOpen(!open)}
      >
        {icon}
        <span>{label}</span>
        {open ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
      </button>
      {open && (
        <div className="filter-popover__content">
          {children}
        </div>
      )}
    </div>
  );
}
