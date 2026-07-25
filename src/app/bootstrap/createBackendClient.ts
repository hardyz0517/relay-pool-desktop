import type { BackendClient } from "@/lib/bridge/BackendClient";
import { createDemoBackendClient } from "@/lib/bridge/DemoBackend";
import { createDesktopBackendClient } from "@/lib/bridge/DesktopBackend";
import { DEMO_BACKEND_MODE, DESKTOP_BACKEND_MODE, type FixedBackendMode } from "./backendMode";

export type BackendClientFactory = () => BackendClient;

export function createBackendClient(mode: FixedBackendMode): BackendClient {
  switch (mode) {
    case DESKTOP_BACKEND_MODE:
      return createDesktopBackendClient();
    case DEMO_BACKEND_MODE:
      return createDemoBackendClient();
    default: {
      throw new Error(`Unsupported backend mode: ${mode}`);
    }
  }
}
