import { useEffect, useRef, type ReactNode } from 'react';

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint ? <span className="hint">{hint}</span> : null}
    </label>
  );
}

export function Panel({ title, children, actions, className }: { title?: ReactNode; children: ReactNode; actions?: ReactNode; className?: string }) {
  return (
    <div className={`panel${className ? ` ${className}` : ''}`}>
      {title || actions ? (
        <div className="panel-head">
          {title ? <h3>{title}</h3> : <span />}
          <div className="spacer" />
          {actions}
        </div>
      ) : null}
      {children}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>;
}

export function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="stat">
      <div className="stat-value">{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}

/** A checkbox that shows "unknown" (indeterminate) when the value was never read. */
export function TriCheck({ value, onChange, title }: { value?: boolean; onChange: (v: boolean) => void; title?: string }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = value === undefined;
  }, [value]);
  return <input ref={ref} type="checkbox" title={title} checked={value ?? false} onChange={(e) => onChange(e.target.checked)} />;
}
