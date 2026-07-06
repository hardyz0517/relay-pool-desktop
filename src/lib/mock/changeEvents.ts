import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

let memoryChangeEvents: ChangeEvent[] = [
  {
    id: "change-demo-balance-low",
    severity: "warning",
    eventType: "balance_low",
    status: "unread",
    title: "余额偏低",
    message: "Orchid Relay 余额低于阈值，可能影响 cheap_first 路由。",
    objectType: "station",
    objectId: "station-orchid",
    stationId: "station-orchid",
    stationKeyId: null,
    pricingRuleId: null,
    requestLogId: null,
    oldValueJson: null,
    newValueJson: JSON.stringify({ value: 4.2, threshold: 10 }),
    impactJson: JSON.stringify({ routingRisk: "deprioritize" }),
    dedupeKey: "balance:low:station:station-orchid",
    source: "balance",
    detectedAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
    resolvedAt: null,
    createdAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
    updatedAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
  },
  {
    id: "change-demo-model-added",
    severity: "info",
    eventType: "model_added",
    status: "unread",
    title: "模型新增",
    message: "Blue Pool 新增模型 gpt-5-mini。",
    objectType: "pricing_rule",
    objectId: "pricing-demo",
    stationId: "station-blue",
    stationKeyId: null,
    pricingRuleId: "pricing-demo",
    requestLogId: null,
    oldValueJson: null,
    newValueJson: JSON.stringify({ model: "gpt-5-mini" }),
    impactJson: null,
    dedupeKey: "model_added:station:station-blue:model:gpt-5-mini",
    source: "collector",
    detectedAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
    resolvedAt: null,
    createdAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
    updatedAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
  },
];

export function listMockChangeEvents() {
  return Promise.resolve([...memoryChangeEvents]);
}

export function clearMockChangeEvents() {
  memoryChangeEvents = [];
  return Promise.resolve();
}

export function upsertMockChangeEvent(input: UpsertChangeEventInput) {
  const now = new Date().toISOString();
  const existingIndex = memoryChangeEvents.findIndex((event) => event.dedupeKey === input.dedupeKey);
  const next: ChangeEvent = {
    id: existingIndex >= 0 ? memoryChangeEvents[existingIndex].id : `change-${Date.now()}`,
    status: "unread",
    detectedAt: now,
    createdAt: existingIndex >= 0 ? memoryChangeEvents[existingIndex].createdAt : now,
    updatedAt: now,
    resolvedAt: null,
    ...input,
  };
  if (existingIndex >= 0) {
    memoryChangeEvents = memoryChangeEvents.map((event, index) => (index === existingIndex ? next : event));
  } else {
    memoryChangeEvents = [next, ...memoryChangeEvents];
  }
  return Promise.resolve(next);
}

export function updateMockChangeEventStatus(id: string, status: ChangeEvent["status"]) {
  const now = new Date().toISOString();
  memoryChangeEvents = memoryChangeEvents.map((event) =>
    event.id === id
      ? { ...event, status, updatedAt: now, resolvedAt: status === "resolved" ? now : event.resolvedAt }
      : event,
  );
  const event = memoryChangeEvents.find((item) => item.id === id);
  if (!event) {
    return Promise.reject(new Error("change event not found"));
  }
  return Promise.resolve(event);
}
