import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { federationRequest } from "../../api";
import { ExpertisePanel } from "./ExpertisePanel";
import type { Discovery } from "./types";
vi.mock("../../api", () => ({ federationRequest: vi.fn() }));
const discovery: Discovery = { space: "space", instance: "local", authority: "authority", principal: "reviewer", permissions: ["memory_read", "bundle_export"], read_only: false, profile: "private" };
const ad = { owner: "remote", collection: "collection", revision: 1, name: "Recovery", description: "Reviewed facts", topics: [] };
const review = { digest: { algorithm: "sha256", value: "digest" }, question: { request_id: "stable-question", requester: "local", principal: "reviewer", capability: { publisher: "remote", id: "collection" }, question: "How to recover?", revision: 1, mode: "deterministic", model_destination: null, output_destination: "local_human", expires_at: "2030-01-01T00:00:00Z" } };
async function prepare() {
  fireEvent.click(screen.getByText("Discover expertise"));
  fireEvent.click(await screen.findByText("Select expertise Recovery"));
  fireEvent.change(screen.getByLabelText("Remote question"), { target: { value: "How to recover?" } });
  fireEvent.click(screen.getByText("Prepare remote question"));
  await screen.findByText("Send approved question");
}
describe("Explicit private expertise", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.mocked(federationRequest).mockReset();
    vi.mocked(federationRequest).mockImplementation(async (_p, _s, path) => {
      if (path?.startsWith("/capabilities")) return { capabilities: [ad], next_after: null };
      if (path === "/questions/prepare") return review;
      return { status: "queued", answer: null, payload_until: "tomorrow" };
    });
  });
  it("does nothing automatically and sends only after exact consent; uncertain retries retain bytes", async () => {
    let attempts = 0;
    vi.mocked(federationRequest).mockImplementation(async (_p, _s, path) => {
      if (path?.startsWith("/capabilities")) return { capabilities: [ad], next_after: null };
      if (path === "/questions/prepare") return review;
      if (path === "/questions/submit" && ++attempts === 1) throw new Error("503 uncertain");
      return { status: "queued", answer: null, payload_until: "tomorrow" };
    });
    render(<ExpertisePanel project="a" discovery={discovery} />);
    expect(federationRequest).not.toHaveBeenCalled();
    await prepare();
    expect(screen.getByText("Send approved question")).toBeDisabled();
    fireEvent.click(screen.getByLabelText("I approve sending these exact question bytes to this instance for local-human output."));
    fireEvent.click(screen.getByText("Send approved question"));
    await screen.findByRole("alert");
    fireEvent.click(screen.getByText("Retry exact question"));
    await screen.findByText(/Question status: queued/);
    const submits = vi.mocked(federationRequest).mock.calls.filter(c => c[2] === "/questions/submit");
    expect(submits).toHaveLength(2);
    expect(submits[0][3]).toEqual({ review, confirmed_digest: review.digest });
    expect(submits[1][3]).toEqual(submits[0][3]);
    const payload = vi.mocked(federationRequest).mock.calls.find(c => c[2] === "/questions/prepare")![3];
    expect(Object.keys(payload as object).sort()).toEqual(["capability", "question", "request_id"]);
  });
  it("renders untrusted answers as text and removes them before a denied refresh", async () => {
    let reads = 0;
    vi.mocked(federationRequest).mockImplementation(async () => {
      if (++reads > 1) throw new Error("403 revoked");
      return { status: "answered", payload_until: "tomorrow", binding: { responder: "remote" }, answer: { text: "<script>steal()</script>", citations: [{ author: "expert", source: { version: 27 } }], insufficient_evidence: false, untrusted: true } };
    });
    render(<ExpertisePanel project="a" discovery={discovery} />);
    fireEvent.change(screen.getByLabelText("Expertise request ID"), { target: { value: "id" } });
    fireEvent.click(screen.getByText("Fetch expertise result"));
    await screen.findByText("<script>steal()</script>");
    expect(document.querySelector("script")).toBeNull();
    expect(screen.getByLabelText("Answer citations and attribution")).toHaveTextContent("expert");
    fireEvent.click(screen.getByText("Fetch expertise result"));
    await screen.findByRole("alert");
    expect(screen.queryByText("<script>steal()</script>")).toBeNull();
  });
  it("defaults to excerpts and binds an explicitly selected synthesis provider into review", async () => {
    const modelReview = { ...review, question: { ...review.question, mode: "model", model_destination: "fixture" } };
    vi.mocked(federationRequest).mockImplementation(async (_p, _s, path) => path?.startsWith("/capabilities")
      ? { capabilities: [{ ...ad, model_destinations: ["fixture"] }], next_after: null } : modelReview);
    render(<ExpertisePanel project="a" discovery={discovery} />);
    expect(screen.getByLabelText("Expertise answer mode")).toHaveValue("");
    fireEvent.click(screen.getByText("Discover expertise"));
    fireEvent.click(await screen.findByText("Select expertise Recovery"));
    fireEvent.change(screen.getByLabelText("Expertise answer mode"), { target: { value: "fixture" } });
    fireEvent.change(screen.getByLabelText("Remote question"), { target: { value: "How to recover?" } });
    fireEvent.click(screen.getByText("Prepare remote question"));
    await screen.findByText("Send approved question");
    expect(vi.mocked(federationRequest).mock.calls[1][3]).toMatchObject({ model_destination: "fixture" });
    expect(screen.getByLabelText(/and synthesis via fixture/)).not.toBeChecked();
    expect(screen.getByText("Send approved question")).toBeDisabled();
    expect(screen.getByLabelText("Expertise answer mode")).toBeDisabled();
    expect(vi.mocked(federationRequest).mock.calls.some(c => c[2] === "/questions/submit")).toBe(false);
  });
  it("clears consent and aborts requests on project or permission changes", async () => {
    const { rerender } = render(<ExpertisePanel project="a" discovery={discovery} />);
    await prepare();
    const signal = vi.mocked(federationRequest).mock.calls[0][4];
    rerender(<ExpertisePanel project="b" discovery={discovery} />);
    expect(signal?.aborted).toBe(true);
    expect(screen.queryByText("Send approved question")).toBeNull();
    expect(screen.getByLabelText("Expertise request ID")).toHaveValue("");
    rerender(<ExpertisePanel project="b" discovery={{ ...discovery, permissions: [] }} />);
    expect(screen.getByText("Discover expertise")).toBeDisabled();
  });
  it("denies read-only operations and bounds UTF-8 questions", async () => {
    const { rerender } = render(<ExpertisePanel project="a" discovery={{ ...discovery, read_only: true }} />);
    expect(screen.getByText("Discover expertise")).toBeDisabled();
    rerender(<ExpertisePanel project="a" discovery={discovery} />);
    fireEvent.click(screen.getByText("Discover expertise"));
    fireEvent.click(await screen.findByText("Select expertise Recovery"));
    fireEvent.change(screen.getByLabelText("Remote question"), { target: { value: "界".repeat(1400) } });
    expect(screen.getByText("Prepare remote question")).toBeDisabled();
    expect(screen.getByText("4200 / 4096 UTF-8 bytes")).toBeTruthy();
  });
  it("follows both cursor fields even when a filtered page is empty", async () => {
    vi.mocked(federationRequest).mockResolvedValue({ capabilities: [], next_after: { publisher: "owner", id: "record" } });
    render(<ExpertisePanel project="a" discovery={discovery} />);
    fireEvent.click(screen.getByText("Discover expertise"));
    await screen.findByText("No permitted expertise on this page.");
    fireEvent.click(screen.getByText("Next expertise page"));
    await waitFor(() => expect(federationRequest).toHaveBeenCalledTimes(2));
    expect(vi.mocked(federationRequest).mock.calls[1][2]).toBe("/capabilities?limit=32&after_publisher=owner&after_record=record");
  });
});
