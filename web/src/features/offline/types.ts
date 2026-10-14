export type LaptopCacheProject = {
  subscription_id: string; device_id: string; company_id: string; area: string;
  owner_instance_id: string; project_id: string; space_id: string; cursor: number;
  observed_at: string | null; initialized: boolean; read_only: true;
  uploads?: Record<string, number>;
  sync_error?: string;
};
export type LaptopCacheList = { schema_version: 1; area: string; instance_id: string; projects: LaptopCacheProject[] };
export type LaptopCacheEvidence = { label: string; text: string; digest: string; reviewer: string; reviewed_at: string };
export type LaptopCacheHit = {
  stream_id: string; author: string; authored_at: string; locator?: string; digest: string;
  shared_version?: number; origin?: "shared_publication" | "curated_project"; source_record_id: string; source_version: number; text: string;
  summary: string; evidence: LaptopCacheEvidence[]; read_only: true;
  unavailable_source_ids?: string[];
};
export type LaptopCacheSearchResponse = { schema_version: 1; project: LaptopCacheProject; hits: LaptopCacheHit[] };
export type LaptopCacheHistoryResponse = {
  schema_version: 1; project: LaptopCacheProject; canonical_id: string; read_only: true; next_after_version: number | null;
  current_state: {importance: number; confidence: number; status: string} | null;
  versions: { id: string; memory_type: import("../../types").MemoryType; version: number; author: string; authored_at: string; updated_at: string; tombstone: boolean; text: string; summary: string; digest: string; unavailable_source_ids: string[] }[];
};
export function sameCacheScope(a: LaptopCacheProject, b: LaptopCacheProject): boolean {
  return a.subscription_id === b.subscription_id && a.device_id === b.device_id
    && a.company_id === b.company_id && a.area === b.area && a.owner_instance_id === b.owner_instance_id
    && a.project_id === b.project_id && a.space_id === b.space_id;
}
