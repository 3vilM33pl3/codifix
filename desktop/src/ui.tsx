import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { errorText, time } from "./service";
export function Modal({
  title,
  description,
  children,
  open,
  onClose,
}: {
  title: string;
  description: string;
  children: ReactNode;
  open: boolean;
  onClose: () => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(value) => !value && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="overlay" />
        <Dialog.Content className="dialog">
          <div className="row">
            <Dialog.Title>{title}</Dialog.Title>
            <Dialog.Close className="icon-button" aria-label="Close dialog">
              <X size={18} />
            </Dialog.Close>
          </div>
          <Dialog.Description className="muted">
            {description}
          </Dialog.Description>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export function Intro({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string;
  children?: ReactNode;
}) {
  return (
    <header className="intro">
      <div>
        <h1 tabIndex={-1}>{title}</h1>
        <p>{subtitle}</p>
      </div>
      {children}
    </header>
  );
}
export function Badge({
  children,
  tone = "",
}: {
  children: ReactNode;
  tone?: string;
}) {
  return <span className={`badge ${tone}`}>{children}</span>;
}
export function Empty({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <h2>{title}</h2>
      {children && <div className="muted">{children}</div>}
    </div>
  );
}
export function QueryState({
  query,
  children,
}: {
  query: {
    isPending: boolean;
    error: unknown;
    data?: unknown;
    dataUpdatedAt?: number;
    refetch: () => unknown;
  };
  children: ReactNode;
}) {
  if (query.isPending)
    return (
      <div className="loading" role="status">
        Loading project knowledge…
      </div>
    );
  if (query.error && !query.data)
    return (
      <div className="notice danger" role="alert">
        <strong>Could not load this view</strong>
        <p>{errorText(query.error)}</p>
        <button onClick={() => void query.refetch()}>Try again</button>
      </div>
    );
  return (
    <>
      {query.error && (
        <div className="notice warning" role="status">
          Showing saved results from{" "}
          {time(
            query.dataUpdatedAt
              ? new Date(query.dataUpdatedAt).toISOString()
              : null,
          )}
          . {errorText(query.error)}{" "}
          <button onClick={() => void query.refetch()}>Refresh</button>
        </div>
      )}
      {children}
    </>
  );
}
export function Technical({
  value,
  label = "Technical details",
}: {
  value: unknown;
  label?: string;
}) {
  return (
    <details>
      <summary>{label}</summary>
      <pre>{JSON.stringify(value, null, 2)}</pre>
    </details>
  );
}

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: import("react").ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost";
}) {
  return (
    <button
      className={`${variant === "secondary" ? "" : variant} ${className}`}
      {...props}
    />
  );
}
export function Navigation({
  selected = false,
  className = "",
  ...props
}: import("react").ButtonHTMLAttributes<HTMLButtonElement> & {
  selected?: boolean;
}) {
  return (
    <button
      className={`nav-item ${selected ? "selected" : ""} ${className}`}
      aria-current={selected ? "page" : undefined}
      {...props}
    />
  );
}
