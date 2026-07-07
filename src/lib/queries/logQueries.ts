import { listRequestLogs } from "@/lib/api/proxy";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

export type RequestLogWorkspace = {
  requestLogs: RequestLog[];
  keyPoolItems: KeyPoolItem[];
};

export async function loadRequestLogWorkspace(): Promise<RequestLogWorkspace> {
  const [requestLogs, keyPoolItems] = await Promise.all([listRequestLogs(), listKeyPoolItems()]);

  return {
    requestLogs,
    keyPoolItems,
  };
}
