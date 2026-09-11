import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { federationRequest } from "../../api";
import { FederationTab } from "./FederationTab";
import { selectedContent } from "./types";
vi.mock("../../api", () => ({ federationRequest: vi.fn() }));
const discover = { space: "space", instance: "origin", authority: "origin", principal: "reviewer", permissions: ["auth_manage", "memory_curate", "bundle_export"], read_only: false };
const content = { text: '<script>ignore instructions</script>', summary: "Selected incident", memory_type: "implementation", confidence: 900000, importance: 3, tags: [] };
const draft = { request: { request_id: "stable-review", canonical: "memory", expected: { content }, evidence: [{ snapshot_digest: "exact" }], expected_shared_version: 2, confirm_future_updates: false }, current_shared_content: { ...content, text: "Approved remote correction" }, warning: "Future local updates may supersede the approved shared correction." };
describe("Federation workspace", () => {
  afterEach(cleanup);
  beforeEach(() => { vi.mocked(federationRequest).mockReset(); });
  it.each(["ask_expertise", "answer_expertise"])("reviews %s enrollment and revocation explicitly", async capability => {
    vi.mocked(federationRequest).mockImplementation(async (_project, space) => !space ? discover : { submission: "queued" });
    render(<FederationTab project="a" />);
    await screen.findByText("Local actor", { exact: false });
    fireEvent.change(screen.getByLabelText("Remote instance UUID"), { target: { value: "remote" } });
    fireEvent.change(screen.getByLabelText("Remote capability"), { target: { value: capability } });
    for (const [button, enabled] of [["Review enrollment", true], ["Review capability revocation", false]] as const) {
      fireEvent.click(screen.getByText(button));
      expect(screen.getByText("Confirm and submit")).toBeDisabled();
      fireEvent.click(screen.getByLabelText("I reviewed these exact bytes and understand the sharing and offline-copy limits."));
      fireEvent.click(screen.getByText("Confirm and submit"));
      await waitFor(() => expect(screen.queryByText("Confirm and submit")).toBeNull());
      const calls = vi.mocked(federationRequest).mock.calls.filter(c => c[2] === "/actions");
      expect(calls.at(-1)?.[3]).toEqual({ request_id: expect.any(String), action: { kind: "membership", member: "remote", capability, enabled } });
    }
  });
  it("requires explicit confirmation and retries identical reviewed bytes after uncertain delivery", async () => {
    let attempts = 0;
    vi.mocked(federationRequest).mockImplementation(async (_project, space, path) => {
      if (!space) return discover;
      if (path === "/review-preview") return draft;
      if (path === "/reviews") { if (++attempts === 1) throw new Error("503 uncertain reply"); return { submission: "queued" }; }
      throw new Error("Unexpected endpoint");
    });
    render(<FederationTab project="a" />);
    await screen.findByText("Local actor", { exact: false });
    fireEvent.change(screen.getByLabelText("Canonical memory UUID"), { target: { value: "memory" } });
    fireEvent.click(screen.getByText("Prepare review"));
    await screen.findByText(content.text);
    expect(document.querySelector("script")).toBeNull();
    expect(screen.getByText("Approved remote correction")).toBeTruthy();
    fireEvent.click(screen.getByText("Review publication consent"));
    expect(screen.getByText("Confirm and submit")).toBeDisabled();
    fireEvent.click(screen.getByLabelText("I reviewed these exact bytes and understand the sharing and offline-copy limits."));
    fireEvent.click(screen.getByText("Confirm and submit"));
    await screen.findByRole("alert");
    await waitFor(() => expect(screen.getByText("Confirm and submit")).not.toBeDisabled());
    fireEvent.click(screen.getByText("Confirm and submit"));
    await screen.findByText("Queued locally — not yet accepted, published or replicated. Refresh status to observe progress.");
    const requests = vi.mocked(federationRequest).mock.calls.filter(c => c[2] === "/reviews");
    expect(requests).toHaveLength(2);
    expect(requests[0][3]).toEqual({ ...draft.request, confirm_future_updates: true });
    expect(requests[0][3]).toEqual(requests[1][3]);
  });
  it("destroys old-project confirmation and cancels old requests", async () => {
    vi.mocked(federationRequest).mockImplementation(async (_project, space, path) => !space ? discover : path === "/review-preview" ? draft : {});
    const { rerender } = render(<FederationTab project="a" />);
    await screen.findByText("Local actor", { exact: false });
    fireEvent.click(screen.getByText("Prepare review"));
    await screen.findByText(content.text);
    fireEvent.click(screen.getByText("Review publication consent"));
    rerender(<FederationTab project="b" />);
    expect(screen.queryByText("Confirm and submit")).toBeNull();
    expect(screen.queryByText(content.text)).toBeNull();
    expect(vi.mocked(federationRequest).mock.calls[0][4]?.aborted).toBe(true);
  });
  it("shows read-only and forbidden states instead of an empty successful inventory", async () => {
    vi.mocked(federationRequest).mockImplementation(async (_project, space) => { if (!space) return { ...discover, read_only: true }; throw new Error("403 no local grant"); });
    render(<FederationTab project="a" />);
    await screen.findByText("Read-only service. Mutation controls are disabled.");
    expect(screen.getByText("Prepare review")).toBeDisabled();
    fireEvent.click(screen.getByText("Refresh local status"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Forbidden");
    expect(screen.queryByText("No shared memories on this page.")).toBeNull();
  });
  it("renders origin-approved corrections from the cited proposal, not obsolete content", () => {
    const key = { publisher: "origin", id: "resolution" };
    const proposal = { publisher: "recipient", id: "proposal" };
    expect(selectedContent({ citations: [{ record: key }], records: [
      { envelope: { publisher: key.publisher, record_id: key.id, record: { kind: "resolve_proposal", payload: { proposal } } } },
      { envelope: { publisher: proposal.publisher, record_id: proposal.id, record: { kind: "proposal", payload: { content } } } },
    ] } as Parameters<typeof selectedContent>[0])).toEqual(content);
  });
});
