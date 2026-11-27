import { useQuery } from "@tanstack/react-query";
import { projectPath, request, type ProjectMemoriesResponse } from "./service";
// The list API is paged. Fetch all pages for local text/type/tag filtering;
// never label the first 200 results as the whole project. Abort between pages
// on project/connection changes, and reject incomplete or mismatched snapshots.
export async function allMemories(
  project: string,
  signal: AbortSignal,
): Promise<ProjectMemoriesResponse> {
  const items: ProjectMemoriesResponse["items"] = [];
  let total = 0;
  do {
    signal.throwIfAborted();
    const page = await request<ProjectMemoriesResponse>(
      projectPath(project, `memories?limit=500&offset=${items.length}`),
    );
    signal.throwIfAborted();
    if (page.project !== project)
      throw new Error(
        "The service returned memories from a different project.",
      );
    total = page.total;
    if (!page.items.length && items.length < total)
      throw new Error(
        "The memory list changed while loading. Refresh to load a complete list.",
      );
    items.push(...page.items);
    if (items.length > 100_000)
      throw new Error(
        "This project exceeds the desktop browse limit. Use the service query interface to search it.",
      );
  } while (items.length < total);
  if (new Set(items.map((m) => m.id)).size !== items.length)
    throw new Error(
      "Memories changed during loading. Refresh to load a complete list.",
    );
  return { project, total, items };
}
export function useMemories(scope: string, project: string) {
  return useQuery({
    queryKey: [scope, project, "all-memories"],
    queryFn: ({ signal }) => allMemories(project, signal),
    retry: false,
    staleTime: 30_000,
  });
}
