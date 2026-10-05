export type LaptopCacheProject = {
  subscription_id: string; device_id: string; company_id: string; area: string;
  owner_instance_id: string; project_id: string; space_id: string; cursor: number;
  observed_at: string | null; initialized: boolean; read_only: true;
};
export type LaptopCacheList = { schema_version: 1; area: string; instance_id: string; projects: LaptopCacheProject[] };
export type LaptopCacheEvidence = { label: string; text: string; digest: string; reviewer: string; reviewed_at: string };
export type LaptopCacheHit = {
  stream_id: string; author: string; authored_at: string; locator: string; digest: string;
  shared_version: number; source_record_id: string; source_version: number; text: string;
  summary: string; evidence: LaptopCacheEvidence[]; read_only: true;
};
export type LaptopCacheSearchResponse = { schema_version: 1; project: LaptopCacheProject; hits: LaptopCacheHit[] };
export function sameCacheScope(a: LaptopCacheProject, b: LaptopCacheProject): boolean {
  return a.subscription_id === b.subscription_id && a.device_id === b.device_id
    && a.company_id === b.company_id && a.area === b.area && a.owner_instance_id === b.owner_instance_id
    && a.project_id === b.project_id && a.space_id === b.space_id;
}
