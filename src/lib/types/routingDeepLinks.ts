import type { RouteEndpointKind } from "@/lib/types/routing";

export type RoutingDeepLink =
  | {
      kind: "station-key";
      stationKeyId: string;
      source?: "key_pool" | "monitoring" | "collector" | "station_endpoint_health";
    }
  | {
      kind: "request";
      requestLogId: string;
      source?: "request_log";
    }
  | {
      kind: "simulate-model";
      model: string;
      endpoint?: RouteEndpointKind;
      source?: "pricing";
    };

export type VersionedRoutingDeepLink = RoutingDeepLink & {
  sequence: number;
};
