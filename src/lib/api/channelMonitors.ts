import { invoke } from "@tauri-apps/api/core";
import type {
  ChannelMonitor,
  ChannelMonitorRequestTemplate,
  ChannelMonitorRun,
  CreateChannelMonitorInput,
  CreateChannelMonitorTemplateInput,
  UpdateChannelMonitorInput,
  UpdateChannelMonitorTemplateInput,
} from "@/lib/types/channelMonitors";

let memoryMonitors: ChannelMonitor[] = [];
let memoryTemplates: ChannelMonitorRequestTemplate[] | null = null;
const memoryRuns = new Map<string, ChannelMonitorRun[]>();

export function listChannelMonitors() {
  return invoke<ChannelMonitor[]>("list_channel_monitors").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryMonitors;
    }
    throw error;
  });
}

export function createChannelMonitor(input: CreateChannelMonitorInput) {
  return invoke<ChannelMonitor>("create_channel_monitor", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const monitor: ChannelMonitor = {
        id: createMemoryId("channel-monitor"),
        ...input,
        createdAt: now,
        updatedAt: now,
      };
      memoryMonitors = [monitor, ...memoryMonitors];
      return monitor;
    }
    throw error;
  });
}

export function updateChannelMonitor(input: UpdateChannelMonitorInput) {
  return invoke<ChannelMonitor>("update_channel_monitor", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const existing = memoryMonitors.find((monitor) => monitor.id === input.id);
      if (!existing) {
        throw new Error(`Channel monitor ${input.id} does not exist in browser preview memory.`);
      }
      const next: ChannelMonitor = {
        ...existing,
        ...input,
        updatedAt: now,
      };
      memoryMonitors = memoryMonitors.map((monitor) => (monitor.id === input.id ? next : monitor));
      return next;
    }
    throw error;
  });
}

export function deleteChannelMonitor(id: string) {
  return invoke<void>("delete_channel_monitor", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memoryMonitors = memoryMonitors.filter((monitor) => monitor.id !== id);
      memoryRuns.delete(id);
      return;
    }
    throw error;
  });
}

export function runChannelMonitorNow(monitorId: string) {
  return invoke<ChannelMonitorRun[]>("run_channel_monitor_now", { monitorId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const monitor = memoryMonitors.find((item) => item.id === monitorId);
      if (!monitor) {
        throw new Error(`Channel monitor ${monitorId} does not exist in browser preview memory.`);
      }
      const now = new Date().toISOString();
      const run: ChannelMonitorRun = {
        id: createMemoryId("channel-monitor-run"),
        monitorId,
        templateId: monitor.templateId,
        stationId: monitor.stationId,
        stationKeyId: monitor.stationKeyId,
        status: "skipped",
        startedAt: now,
        finishedAt: now,
        durationMs: 0,
        httpStatus: null,
        latencyMs: null,
        responseModel: null,
        fallbackModel: null,
        errorMessage: "Browser preview fallback only; no real channel probe or scheduler ran.",
        createdAt: now,
      };
      memoryRuns.set(monitorId, [run, ...(memoryRuns.get(monitorId) ?? [])]);
      return [run];
    }
    throw error;
  });
}

export function listChannelMonitorRuns(monitorId: string) {
  return invoke<ChannelMonitorRun[]>("list_channel_monitor_runs", { monitorId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRuns.get(monitorId) ?? [];
    }
    throw error;
  });
}

export function listChannelMonitorTemplates() {
  return invoke<ChannelMonitorRequestTemplate[]>("list_channel_monitor_templates").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return ensureMemoryTemplates();
    }
    throw error;
  });
}

export function createChannelMonitorTemplate(input: CreateChannelMonitorTemplateInput) {
  return invoke<ChannelMonitorRequestTemplate>("create_channel_monitor_template", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const template: ChannelMonitorRequestTemplate = {
        id: createMemoryId("channel-monitor-template"),
        ...input,
        builtIn: false,
        createdAt: now,
        updatedAt: now,
      };
      memoryTemplates = [template, ...ensureMemoryTemplates()];
      return template;
    }
    throw error;
  });
}

export function updateChannelMonitorTemplate(input: UpdateChannelMonitorTemplateInput) {
  return invoke<ChannelMonitorRequestTemplate>("update_channel_monitor_template", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const existing = ensureMemoryTemplates().find((template) => template.id === input.id);
      if (!existing) {
        throw new Error(`Channel monitor template ${input.id} does not exist in browser preview memory.`);
      }
      const next: ChannelMonitorRequestTemplate = {
        ...existing,
        ...input,
        builtIn: false,
        updatedAt: now,
      };
      memoryTemplates = ensureMemoryTemplates().map((template) => (template.id === input.id ? next : template));
      return next;
    }
    throw error;
  });
}

export function duplicateChannelMonitorTemplate(id: string) {
  return invoke<ChannelMonitorRequestTemplate>("duplicate_channel_monitor_template", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const source = ensureMemoryTemplates().find((template) => template.id === id);
      if (!source) {
        throw new Error(`Channel monitor template ${id} does not exist in browser preview memory.`);
      }
      const now = new Date().toISOString();
      const copy: ChannelMonitorRequestTemplate = {
        ...source,
        id: createMemoryId("channel-monitor-template"),
        name: `${source.name} Copy`,
        builtIn: false,
        createdAt: now,
        updatedAt: now,
      };
      memoryTemplates = [copy, ...ensureMemoryTemplates()];
      return copy;
    }
    throw error;
  });
}

export function deleteChannelMonitorTemplate(id: string) {
  return invoke<void>("delete_channel_monitor_template", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memoryTemplates = ensureMemoryTemplates().filter((template) => template.id !== id || template.builtIn);
      return;
    }
    throw error;
  });
}

function ensureMemoryTemplates() {
  if (memoryTemplates) {
    return memoryTemplates;
  }
  const now = new Date().toISOString();
  memoryTemplates = [
    {
      id: "preview-openai-chat-default",
      name: "Preview OpenAI Chat Probe",
      endpointKind: "chat_completions",
      method: "POST",
      path: "/v1/chat/completions",
      requestBodyJson: JSON.stringify(
        {
          model: "{{model}}",
          messages: [{ role: "user", content: "{{challenge}}" }],
          max_tokens: 1,
          stream: false,
        },
        null,
        2,
      ),
      enabled: true,
      builtIn: true,
      note: "Browser preview template; real templates are stored by the Tauri backend.",
      createdAt: now,
      updatedAt: now,
    },
  ];
  return memoryTemplates;
}

function createMemoryId(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
