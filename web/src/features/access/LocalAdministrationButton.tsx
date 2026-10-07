import { useState } from "react";
import { openLocalAdministration } from "../../api";
import type { AuthMeResponse } from "../../types";

export function LocalAdministrationButton({ auth, onOpened }: {
  auth: AuthMeResponse; onOpened: (auth: AuthMeResponse) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  if (auth.mode !== "single_user" || auth.principal.kind !== "internal" || auth.read_only) return null;
  async function open() {
    setBusy(true);
    setError(false);
    try { onOpened(await openLocalAdministration()); }
    catch { setError(true); }
    finally { setBusy(false); }
  }
  return <>
    <button type="button" disabled={busy} onClick={() => void open()}>
      {busy ? "Opening administration…" : "Open administration"}
    </button>
    {error ? <span role="alert">Administration login failed. Check that this is a writable, local single-user service and try again.</span> : null}
  </>;
}
