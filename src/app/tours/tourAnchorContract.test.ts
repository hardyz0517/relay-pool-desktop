import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { AppPageId } from "@/lib/types/navigation";
import { PUBLISHED_TOURS } from "./tourCatalog";

type AnchorContract = {
  source: string;
  scope: "page" | "global";
};

const sourceText = (relativePath: string): string =>
  readFileSync(new URL(relativePath, import.meta.url), "utf8");

const appShellSource = sourceText("../../components/shell/AppShell.tsx");
const shellPageHostSource = sourceText("../ShellPageHost.tsx");

const pageSources: Partial<Record<AppPageId, string>> = {
  dashboard: sourceText("../../features/dashboard/DashboardPage.tsx"),
  settings: sourceText("../../features/settings/SettingsPage.tsx"),
  stations: sourceText("../../features/stations/StationsPage.tsx"),
  keyPool: sourceText("../../features/key-pool/KeyPoolPage.tsx"),
  routing: sourceText("../../features/routing/RoutingPage.tsx") +
    sourceText("../../features/routing/LocalRoutingEditTab.tsx") +
    sourceText("../../features/routing/LocalRoutingSettingsEditor.tsx"),
  pricing: sourceText("../../features/pricing/PricingPage.tsx"),
  channels: sourceText("../../features/channels/ChannelStatusPage.tsx") +
    sourceText("../../features/channels/ChannelStatusTab.tsx") +
    sourceText("../../features/channels/OfficialStatusTab.tsx"),
  changes: sourceText("../../features/changes/ChangeCenterPage.tsx"),
  logs: sourceText("../../features/logs/LogsPage.tsx"),
  collectors: sourceText("../../features/collectors/CollectorsPage.tsx"),
};

const anchorContracts: Record<string, AnchorContract> = {
  "shell-sidebar": { source: appShellSource, scope: "global" },
  "nav-dashboard": { source: appShellSource, scope: "global" },
  "nav-stations": { source: appShellSource, scope: "global" },
  "nav-key-pool": { source: appShellSource, scope: "global" },
  "nav-routing": { source: appShellSource, scope: "global" },
  "nav-pricing": { source: appShellSource, scope: "global" },
  "nav-channels": { source: appShellSource, scope: "global" },
  "nav-changes": { source: appShellSource, scope: "global" },
  "nav-logs": { source: appShellSource, scope: "global" },
  "nav-settings": { source: appShellSource, scope: "global" },
};

function hasAnchorLiteral(source: string, anchor: string): boolean {
  return source.includes(`"${anchor}"`) || source.includes(`'${anchor}'`);
}

describe("published tour anchor contract", () => {
  it("maps every published step to a real page or global shell anchor", () => {
    for (const tour of PUBLISHED_TOURS) {
      for (const step of tour.steps) {
        const contract = anchorContracts[step.target.anchor];
        const source = contract?.source ?? pageSources[step.route];

        expect(source, `${tour.id}/${step.id} has a source mapping`).toBeDefined();
        expect(
          hasAnchorLiteral(source ?? "", step.target.anchor),
          `${tour.id}/${step.id} anchor ${step.target.anchor} is declared by its source`,
        ).toBe(true);

        if (contract) {
          expect(contract.scope).toBe("global");
        } else {
          expect(step.route).not.toBe("addProvider");
          expect(step.route).not.toBe("addKey");
        }
      }
    }
  });

  it("keeps global navigation anchors explicitly discoverable outside shell page layers", () => {
    expect(appShellSource).toContain('data-tour-scope="global"');
    expect(appShellSource).toContain("data-tour={tourNavigationAnchors[route.id]}");
    expect(appShellSource).toContain('data-tour="shell-sidebar"');
  });

  it("keeps page anchors behind the active-layer visibility contract", () => {
    expect(shellPageHostSource).toContain("data-page-transition-layer");
    expect(shellPageHostSource).toContain("data-page-transition-state={state}");
    expect(shellPageHostSource).toContain('inert={inert ? "" : undefined}');
  });
});
