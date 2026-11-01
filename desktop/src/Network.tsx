import { useEffect, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge, Button, Empty, Intro, Technical } from "./ui";
import { errorText, request, time, type AuthMeResponse } from "./service";
import {
  canContact,
  featureUnavailable,
  networkStatus,
  openIdentityAttempt,
  specialists,
  type IdentityAttempt,
  type NetworkStatus,
  type Specialist,
} from "./network-api";

export type NetworkProps = {
  scope: string;
  auth: AuthMeResponse | null;
  connected: boolean;
  onConnect: () => void;
};
export function networkKey(scope: string, principal: string) {
  return [scope, "network", principal];
}
export function useMounted() {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return mounted;
}

export function NetworkGate({
  scope,
  auth,
  connected,
  onConnect,
  children,
}: NetworkProps & {
  children: (status: NetworkStatus, auth: AuthMeResponse) => ReactNode;
}) {
  const human = !!auth?.authenticated && auth.principal.kind === "human_oidc";
  const status = useQuery({
    queryKey: [
      ...networkKey(scope, auth?.principal.id ?? "signed-out"),
      "status",
    ],
    queryFn: networkStatus,
    enabled: human && connected,
    retry: false,
    refetchInterval: connected ? 30_000 : false,
  });
  if (!human)
    return (
      <Empty title="Sign in as yourself">
        <p>Your private conversations belong to your Codifix human account.</p>
        <Button className="primary" onClick={onConnect}>
          Sign in
        </Button>
      </Empty>
    );
  if (!connected && !status.data)
    return (
      <Empty title="Reconnect to load your network">
        <Button onClick={onConnect}>Reconnect</Button>
      </Empty>
    );
  if (featureUnavailable(status.error) || status.data?.enabled === false)
    return (
      <Empty title="Network and Inbox are not enabled">
        <p>
          This service has not enabled private specialist conversations. Your
          existing project tools remain available.
        </p>
      </Empty>
    );
  if (status.isPending) return <p role="status">Loading your network…</p>;
  if (status.error || !status.data)
    return (
      <div className="notice danger" role="alert">
        <p>{errorText(status.error)}</p>
        <Button onClick={() => void status.refetch()}>Try again</Button>
      </div>
    );
  return <>{children(status.data, auth!)}</>;
}

export function SpecialistCard({
  specialist,
  children,
}: {
  specialist: Specialist;
  children?: ReactNode;
}) {
  return (
    <article className="card specialist-card">
      <div className="row">
        <h3>{specialist.display_name}</h3>
        <Badge tone={canContact(specialist) ? "positive" : ""}>
          {specialist.enrolled ? "Enrolled" : "Discovered"}
        </Badge>
      </div>
      <p>{specialist.summary}</p>
      <div className="actions">
        {specialist.capabilities.map((capability) => (
          <Badge key={capability}>{capability.replaceAll("_", " ")}</Badge>
        ))}
      </div>
      <p className="caption">
        {specialist.remote ? "Remote instance" : "This instance"} ·{" "}
        {specialist.available ? "Available" : "Unavailable"}
        {specialist.enrolled && !specialist.grant_id
          ? " · No invocation grant"
          : ""}
      </p>
      {children}
    </article>
  );
}

export type DiscoveredProfile = {
  did: string;
  handle: string | null;
  pds: string;
  uri: string;
  cid: string;
  fetched_at: string;
  profile: {
    instanceId: string;
    specialistId: string;
    displayName: string;
    summary: string;
    capabilities: string[];
    agentCardUrl: string;
  };
};

export function Network(props: NetworkProps & { onInbox: () => void }) {
  return (
    <>
      <Intro
        title="Network"
        subtitle="Find specialists and connect your public identity."
      />
      <NetworkGate {...props}>
        {(status, auth) => (
          <NetworkContents
            key={`${props.scope}:${auth.principal.id}`}
            {...props}
            auth={auth}
            status={status}
          />
        )}
      </NetworkGate>
    </>
  );
}

function NetworkContents({
  scope,
  auth,
  connected,
  status,
  onInbox,
}: NetworkProps & {
  auth: AuthMeResponse;
  status: NetworkStatus;
  onInbox: () => void;
}) {
  const key = networkKey(scope, auth.principal.id);
  const client = useQueryClient();
  const mounted = useMounted();
  const [identifier, setIdentifier] = useState("");
  const [handle, setHandle] = useState("");
  const [profiles, setProfiles] = useState<DiscoveredProfile[] | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState("");
  const attemptKey = [...key, "identity-attempt"];
  const attemptState = useQuery<IdentityAttempt | null>({
    queryKey: attemptKey,
    queryFn: async () => null,
    initialData: null,
    enabled: false,
    gcTime: 10 * 60_000,
  });
  const attempt = attemptState.data ?? null;
  const setAttempt = (value: IdentityAttempt | null) =>
    client.setQueryData(attemptKey, value);
  const [unlinkReview, setUnlinkReview] = useState(false);
  const roster = useQuery({
    queryKey: [...key, "specialists"],
    queryFn: specialists,
    enabled: connected,
    retry: false,
    refetchInterval: connected ? 15_000 : false,
  });
  const locked = !connected || auth.read_only || !!busy;
  useEffect(() => {
    if (!attempt || attempt.status !== "pending" || !connected) return;
    let cancelled = false;
    let timeout: ReturnType<typeof setTimeout>;
    const poll = async () => {
      if (Date.parse(attempt.expires_at) <= Date.now()) {
        if (!cancelled)
          setAttempt({
            ...attempt,
            status: "expired",
            authorization_url: null,
          });
        return;
      }
      try {
        const next = await request<IdentityAttempt>(
          `/v1/network/identity/link/${attempt.attempt_id}`,
        );
        if (cancelled) return;
        if (next.attempt_id !== attempt.attempt_id)
          throw new Error(
            "The account-link response did not match this attempt.",
          );
        setAttempt(next);
        if (next.status === "complete") {
          setError("");
          setReceipt(
            "AT identity linked. Your existing permissions are unchanged.",
          );
          await client.invalidateQueries({ queryKey: [...key, "status"] });
        } else if (next.status === "pending") {
          timeout = setTimeout(() => void poll(), 2000);
        }
      } catch (cause) {
        if (!cancelled) {
          setError(errorText(cause));
          timeout = setTimeout(() => void poll(), 5000);
        }
      }
    };
    timeout = setTimeout(() => void poll(), 1500);
    return () => {
      cancelled = true;
      clearTimeout(timeout);
    };
    // This loop follows the current attempt, account and connection only.
  }, [
    attempt?.attempt_id,
    attempt?.status,
    connected,
    scope,
    auth.principal.id,
  ]);

  async function act(name: string, action: () => Promise<void>) {
    setBusy(name);
    setError("");
    setReceipt("");
    try {
      await action();
    } catch (cause) {
      if (mounted.current) setError(errorText(cause));
    } finally {
      if (mounted.current) setBusy("");
    }
  }
  return (
    <div className="network-stack">
      <section className="card">
        <div className="row">
          <h2>Your AT identity</h2>
          <Badge>{status.identity ? "Linked" : "Optional"}</Badge>
        </div>
        {status.identity ? (
          <>
            <p>
              <strong>{status.identity.handle || status.identity.did}</strong>
            </p>
            <p className="caption">Linked {time(status.identity.linked_at)}</p>
            <p className="muted">
              Your identity helps other participants recognize you. Private
              access comes from your Codifix grants.
            </p>
            {unlinkReview ? (
              <div className="notice">
                <p>
                  Unlink this public identity? Your conversations and Codifix
                  permissions will remain.
                </p>
                <div className="actions">
                  <Button
                    disabled={locked}
                    onClick={() =>
                      void act("unlink", async () => {
                        await request("/v1/network/identity/unlink", {});
                        if (!mounted.current) return;
                        setUnlinkReview(false);
                        setAttempt(null);
                        setReceipt(
                          "AT identity unlinked. Conversation history is preserved.",
                        );
                        await client.invalidateQueries({
                          queryKey: [...key, "status"],
                        });
                      })
                    }
                  >
                    {busy === "unlink" ? "Unlinking…" : "Confirm unlink"}
                  </Button>
                  <Button
                    disabled={locked}
                    onClick={() => setUnlinkReview(false)}
                  >
                    Keep linked
                  </Button>
                </div>
              </div>
            ) : (
              <Button disabled={locked} onClick={() => setUnlinkReview(true)}>
                Unlink identity
              </Button>
            )}
          </>
        ) : status.identity_linking_available ? (
          <form
            className="form"
            onSubmit={(event) => {
              event.preventDefault();
              void act("link", async () => {
                const next = await request<IdentityAttempt>(
                  "/v1/network/identity/link",
                  { identifier: handle.trim() },
                );
                if (!mounted.current) return;
                setAttempt(next);
                if (next.status === "pending")
                  await openIdentityAttempt(next.attempt_id);
              });
            }}
          >
            <p className="muted">
              Keep your Codifix sign-in and link an AT account through your
              browser. Linking is optional for private conversations.
            </p>
            <label>
              AT handle or DID
              <input
                value={handle}
                onChange={(event) => setHandle(event.target.value)}
                placeholder="you.example.com"
                disabled={locked || attempt?.status === "pending"}
                required
              />
            </label>
            <Button
              type="submit"
              disabled={
                locked || !handle.trim() || attempt?.status === "pending"
              }
            >
              {busy === "link" ? "Preparing account link…" : "Link AT identity"}
            </Button>
          </form>
        ) : (
          <p className="muted">
            Account linking is not configured on this service. You can still use
            your enrolled specialists.
          </p>
        )}
        {attempt?.status === "pending" && (
          <div className="notice" role="status">
            <strong>Finish linking in your browser</strong>
            <p>
              This attempt expires {time(attempt.expires_at)}. This page checks
              for completion automatically.
            </p>
            <Button
              disabled={locked}
              onClick={() =>
                void act("open", () => openIdentityAttempt(attempt.attempt_id))
              }
            >
              Open browser again
            </Button>
          </div>
        )}
        {attempt &&
          ["expired", "failed", "cancelled"].includes(attempt.status) && (
            <p className="notice warning" role="status">
              Account linking {attempt.status}.{" "}
              {attempt.error || "Start a new attempt when ready."}
            </p>
          )}
      </section>
      {error && (
        <div className="notice danger" role="alert">
          {error}
        </div>
      )}
      {receipt && (
        <div className="notice positive" role="status">
          {receipt}
        </div>
      )}
      <section className="network-stack">
        <div className="row">
          <h2>Your specialists</h2>
          <Button onClick={onInbox}>Open Inbox</Button>
        </div>
        {roster.isPending ? (
          <p role="status">Loading specialists…</p>
        ) : roster.error ? (
          <div role="alert" className="notice danger">
            {errorText(roster.error)}{" "}
            <Button onClick={() => void roster.refetch()}>Refresh</Button>
          </div>
        ) : !roster.data?.length ? (
          <Empty title="No specialists enrolled">
            <p>
              An operator can enroll specialists and grant you access to their
              mandates.
            </p>
          </Empty>
        ) : (
          <div className="specialist-grid">
            {roster.data.map((specialist) => (
              <SpecialistCard specialist={specialist} key={specialist.id} />
            ))}
          </div>
        )}
      </section>
      <section className="card">
        <h2>Discover a public profile</h2>
        <p className="muted">
          Look up a handle, DID or capability record. Discovery verifies its
          published identity; an operator must separately enroll it before
          private work.
        </p>
        <form
          className="network-discovery"
          onSubmit={(event) => {
            event.preventDefault();
            void act("discover", async () => {
              const found = await request<{ profiles: DiscoveredProfile[] }>(
                "/v1/network/discover",
                { identifier: identifier.trim() },
              );
              if (mounted.current) setProfiles(found.profiles);
            });
          }}
        >
          <label>
            Handle, DID or AT record
            <input
              value={identifier}
              onChange={(event) => setIdentifier(event.target.value)}
              placeholder="specialist.example.com"
              disabled={locked}
              required
            />
          </label>
          <Button type="submit" disabled={locked || !identifier.trim()}>
            {busy === "discover" ? "Resolving identity…" : "Discover"}
          </Button>
        </form>
        {profiles && !profiles.length && (
          <p role="status">
            No Codifix capability profiles were published by this identity.
          </p>
        )}
        {profiles?.map((entry) => (
          <article key={entry.uri} className="discovered-profile">
            <div className="row">
              <h3>{entry.profile.displayName}</h3>
              <Badge>Public identity verified</Badge>
            </div>
            <p>{entry.profile.summary}</p>
            <p className="caption">
              Public discovery does not grant private access. Enrollment is
              managed separately.
            </p>
            <p className="caption">
              {entry.handle || entry.did} · Retrieved {time(entry.fetched_at)}
            </p>
            <div className="actions">
              {entry.profile.capabilities.map((c) => (
                <Badge key={c}>{c}</Badge>
              ))}
            </div>
            <Technical
              label="Public record provenance"
              value={{
                did: entry.did,
                pds: entry.pds,
                uri: entry.uri,
                cid: entry.cid,
                fetched_at: entry.fetched_at,
              }}
            />
          </article>
        ))}
      </section>
    </div>
  );
}
