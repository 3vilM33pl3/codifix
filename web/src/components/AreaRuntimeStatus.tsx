import type { RuntimeStatusResponse } from "../types";

type Status = Pick<RuntimeStatusResponse, "generated_at" | "area" | "federation_configured">;

export function AreaRuntimeStatus({ status, onOpenFederation, now = Date.now() }: {
  status: Status | null;
  onOpenFederation: () => void;
  now?: number;
}) {
  const observed = status ? Date.parse(status.generated_at) : NaN;
  const fresh = Number.isFinite(observed) && now >= observed - 5_000 && now - observed <= 15_000;
  const federation = !fresh || status?.federation_configured === undefined
    ? "unknown"
    : status.federation_configured ? "configured" : "disabled";
  return <>
    <span title={status?.area?.instance_id}>Area {status?.area?.name ?? "unassigned"}</span>
    {status?.area ? <span>Instance {status.area.instance_id}</span> : null}
    <button type="button" onClick={onOpenFederation}
      title="Configuration is not proof of synchronization. Open Federation for current project status.">
      Federation {federation}
    </button>
  </>;
}
