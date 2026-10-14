import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { laptopCacheList, laptopCacheSearch } from "../../api";
import { OfflineTab } from "./OfflineTab";
import type { LaptopCacheProject, LaptopCacheSearchResponse } from "./types";
vi.mock("../../api", () => ({ laptopCacheList: vi.fn(), laptopCacheSearch: vi.fn() }));
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const project: LaptopCacheProject = { subscription_id: uuid(1), project_id: uuid(2), company_id: uuid(3), device_id: uuid(4), owner_instance_id: uuid(5), space_id: uuid(6), area: "workshop", cursor: 3, initialized: true, observed_at: null, read_only: true };
const second = { ...project, subscription_id: uuid(7), owner_instance_id: uuid(8) };
const inventory = { schema_version: 1 as const, area: "workshop", instance_id: uuid(9), projects: [project, second] };
const result: LaptopCacheSearchResponse = { schema_version: 1, project, hits: [{ stream_id: uuid(10), author: "owner", authored_at: "2026-10-05T12:00:00Z", locator: "javascript:alert(1)", digest: "a".repeat(64), shared_version: 2, source_record_id: uuid(11), source_version: 4, text: "<script>untrusted excerpt</script>", summary: "Shared summary", evidence: [{ label: "source", text: "Reviewed evidence", digest: "b".repeat(64), reviewer: "reviewer", reviewed_at: "2026-10-04T12:00:00Z" }], read_only: true }] };
afterEach(cleanup);
beforeEach(() => { vi.mocked(laptopCacheList).mockReset().mockResolvedValue(inventory); vi.mocked(laptopCacheSearch).mockReset().mockResolvedValue(result); });
async function submit() {
  await screen.findByLabelText("Local search question");
  fireEvent.change(screen.getByLabelText("Local search question"), { target: { value: "Exact local question" } });
  fireEvent.click(screen.getByRole("button", { name: "Search local cache" }));
}
it("shows sanitized contribution progress without claiming proposals are accepted", async () => {
  vi.mocked(laptopCacheList).mockResolvedValue({...inventory,projects:[{...project,uploads:{queued:2,proposed:1,blocked:1}}]});
  render(<OfflineTab />);
  const status=await screen.findByText(/Curated uploads:/);
  expect(status).toHaveTextContent("queued: 2");
  expect(status).toHaveTextContent("proposed: 1");
  expect(status).toHaveTextContent("need owner review");
});
it("shows private curated versions without implying an ATProto publication", async () => {
  vi.mocked(laptopCacheSearch).mockResolvedValue({ ...result, hits: [{ ...result.hits[0], origin: "curated_project", locator: undefined, shared_version: undefined, unavailable_source_ids: [uuid(20)], evidence: [] }] });
  render(<OfflineTab />);
  await submit();
  const panel = await screen.findByLabelText("Cache search results");
  expect(panel).toHaveTextContent("curated project replica");
  expect(panel).toHaveTextContent("no publication locator");
  expect(panel).toHaveTextContent("source content is not downloaded");
  expect(panel).not.toHaveTextContent("shared version");
});
it("selects exact subscription and renders plain text with complete provenance, never links or mutation controls", async () => {
  const { container } = render(<OfflineTab />);
  await submit(); await screen.findByText("<script>untrusted excerpt</script>");
  expect(laptopCacheSearch).toHaveBeenCalledWith(project.subscription_id, "Exact local question", 8, expect.any(AbortSignal));
  expect(screen.getByLabelText("Cache subscription")).toHaveValue(project.subscription_id);
  expect(screen.getByLabelText("Cache search results")).toHaveTextContent("source version 4");
  expect(screen.getByLabelText("Cache search results")).toHaveTextContent("shared version 2");
  expect(screen.getByLabelText("Cache search results")).toHaveTextContent("reviewer reviewer");
  expect(screen.getByText(/Last observed sync/)).toHaveTextContent("unknown");
  expect(screen.getByText(/Offline copies may be stale/)).toHaveTextContent("not current remote authority");
  expect(container.querySelector("script, a")).toBeNull();
  expect(screen.queryByRole("button", { name: /approve|edit|delete|forward/i })).toBeNull();
});
it("clears and fences late search replies when the subscription changes", async () => {
  let resolve!: (value: LaptopCacheSearchResponse) => void;
  vi.mocked(laptopCacheSearch).mockReturnValue(new Promise(r => { resolve = r; }));
  render(<OfflineTab />); await submit();
  const signal = vi.mocked(laptopCacheSearch).mock.calls[0][3];
  fireEvent.change(screen.getByLabelText("Cache subscription"), { target: { value: second.subscription_id } });
  expect(signal?.aborted).toBe(true);
  resolve(result); await waitFor(() => expect(screen.queryByLabelText("Cache search results")).toBeNull());
  expect(screen.getByLabelText("Local search question")).toHaveValue("");
});
it("refresh drops previous content even when the new inventory fails", async () => {
  render(<OfflineTab />); await submit(); await screen.findByText("Shared summary", { exact: false });
  vi.mocked(laptopCacheList).mockRejectedValue(new Error("Forbidden local owner"));
  fireEvent.click(screen.getByText("Refresh synchronized projects"));
  expect(screen.queryByLabelText("Cache search results")).toBeNull();
  expect(await screen.findByRole("alert")).toHaveTextContent("Forbidden");
  expect(screen.queryByText(/No configured synchronized projects/)).toBeNull();
});
it("rejects a reply with changed stable owner identity", async () => {
  vi.mocked(laptopCacheSearch).mockResolvedValue({ ...result, project: second });
  render(<OfflineTab />); await submit();
  expect(await screen.findByRole("alert")).toHaveTextContent("Cache scope changed");
  expect(screen.queryByLabelText("Cache search results")).toBeNull();
});
it("clears excerpts for edited query and refuses oversized or uninitialized searches", async () => {
  render(<OfflineTab />); await submit(); await screen.findByLabelText("Cache search results");
  fireEvent.change(screen.getByLabelText("Local search question"), { target: { value: "é".repeat(2049) } });
  expect(screen.queryByLabelText("Cache search results")).toBeNull();
  expect(screen.getByRole("button", { name: "Search local cache" })).toBeDisabled();
  cleanup(); vi.mocked(laptopCacheList).mockResolvedValue({ ...inventory, projects: [{ ...project, initialized: false }] });
  render(<OfflineTab />); await screen.findByLabelText("Local search question");
  expect(screen.getByRole("button", { name: "Search local cache" })).toBeDisabled();
});
it("aborts pending reads and searches on unmount", async () => {
  vi.mocked(laptopCacheSearch).mockReturnValue(new Promise(() => {}));
  const { unmount } = render(<OfflineTab />); await submit(); unmount();
  expect(vi.mocked(laptopCacheList).mock.calls[0][0]?.aborted).toBe(true);
  expect(vi.mocked(laptopCacheSearch).mock.calls[0][3]?.aborted).toBe(true);
});
it("search errors do not display earlier excerpts or imply success", async () => {
  render(<OfflineTab />); await submit(); await screen.findByLabelText("Cache search results");
  vi.mocked(laptopCacheSearch).mockRejectedValue(new Error("Synchronized cache unavailable"));
  fireEvent.click(screen.getByRole("button", { name: "Search local cache" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("unavailable");
  expect(screen.queryByLabelText("Cache search results")).toBeNull();
});
