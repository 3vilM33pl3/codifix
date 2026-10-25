import { useQuery } from "@tanstack/react-query";
import { request, projectPath } from "./service";
export function useProjectData<T>(
  scope: string,
  project: string,
  suffix: string,
  enabled = true,
) {
  return useQuery({
    queryKey: [scope, project, suffix],
    queryFn: () => request<T>(projectPath(project, suffix)),
    enabled: enabled && !!project,
    retry: false,
    refetchInterval: 30_000,
  });
}
