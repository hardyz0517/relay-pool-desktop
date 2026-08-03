import type { RouteEndpointKind } from "@/lib/types/routing";

export type RoutingDeepLink =
  | {
      kind: "station";
      stationId: string;
      source?: "collector" | "station_endpoint_health" | "change_center" | "pricing";
    }
  | {
      kind: "station-key";
      stationKeyId: string;
      source?: "key_pool" | "monitoring" | "collector" | "station_endpoint_health" | "change_center" | "pricing";
    }
  | {
      kind: "request";
      requestLogId: string;
      source?: "request_log" | "change_center";
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
