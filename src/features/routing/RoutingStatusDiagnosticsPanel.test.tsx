// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { RoutingStatusDiagnosticsPanel, sortFailureDomainDiagnostics } from "./RoutingStatusDiagnosticsPanel";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("routing Provider/failure-domain diagnostics", () => {
  it("is hidden outside developer mode", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={null}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          developerModeEnabled={false}
        />,
      );
    });
    expect(host.textContent).toBe("");
    act(() => root.unmount());
  });

  it("sorts by candidate count without changing backend protection facts", () => {
    const diagnostics = sortFailureDomainDiagnostics([
      {
        commitment: "v1:smaller",
        resolution: "resolved",
        providerFamily: "openai",
        deploymentIdentity: "backup",
        regionIdentity: "us",
        revision: 2,
        candidateCount: 1,
        schedulableCandidateCount: 0,
        status: "open",
        persistenceKind: "runtime_capacity",
        recentFailureCode: "capacity_exhausted",
        explanationKey: "routing.protection.open",
      },
      {
        commitment: "v1:larger",
        resolution: "resolved",
        providerFamily: "openai",
        deploymentIdentity: "primary",
        regionIdentity: "us",
        revision: 3,
        candidateCount: 3,
        schedulableCandidateCount: 2,
        status: "degraded",
        persistenceKind: "durable",
        recentFailureCode: "upstream_5xx",
        explanationKey: "routing.protection.closed_monitoring",
      },
    ]);

    expect(diagnostics.map((item) => item.commitment)).toEqual([
      "v1:larger",
      "v1:smaller",
    ]);
    expect(diagnostics[0]).toMatchObject({
      candidateCount: 3,
      schedulableCandidateCount: 2,
      status: "degraded",
      recentFailureCode: "upstream_5xx",
      explanationKey: "routing.protection.closed_monitoring",
    });
  });

  it("keeps unresolved identity groups visible and never invents a commitment", () => {
    const diagnostics = sortFailureDomainDiagnostics([
      {
        commitment: null,
        resolution: "model_required",
        providerFamily: "openai",
        deploymentIdentity: "primary",
        regionIdentity: null,
        revision: 1,
        candidateCount: 2,
        schedulableCandidateCount: 2,
        status: "no_protection",
        persistenceKind: null,
        recentFailureCode: null,
        explanationKey: "routing.failure_domain.model_required",
      },
    ]);

    expect(diagnostics).toEqual([
      expect.objectContaining({
        commitment: null,
        resolution: "model_required",
        candidateCount: 2,
      }),
    ]);
  });
});
