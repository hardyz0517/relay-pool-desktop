import type { StationGroupVisualPlatform } from "./groupVisualMeta";

export const groupVisualClassNames: Record<
  StationGroupVisualPlatform,
  {
    badge: string;
    icon: string;
    rateBadge: string;
  }
> = {
  anthropic: {
    badge: "border-platform-anthropic-border bg-platform-anthropic-surface text-platform-anthropic-foreground",
    icon: "text-platform-anthropic-foreground",
    rateBadge: "bg-platform-anthropic-surface text-platform-anthropic-foreground",
  },
  openai: {
    badge: "border-platform-openai-border bg-platform-openai-surface text-platform-openai-foreground",
    icon: "text-platform-openai-foreground",
    rateBadge: "bg-platform-openai-surface text-platform-openai-foreground",
  },
  gemini: {
    badge: "border-platform-gemini-border bg-platform-gemini-surface text-platform-gemini-foreground",
    icon: "text-platform-gemini-foreground",
    rateBadge: "bg-platform-gemini-surface text-platform-gemini-foreground",
  },
  grok: {
    badge: "border-platform-grok-border bg-platform-grok-surface text-platform-grok-foreground",
    icon: "text-platform-grok-foreground",
    rateBadge: "bg-platform-grok-surface text-platform-grok-foreground",
  },
  image: {
    badge: "border-platform-image-border bg-platform-image-surface text-platform-image-foreground",
    icon: "text-platform-image-foreground",
    rateBadge: "bg-platform-image-surface text-platform-image-foreground",
  },
  generic: {
    badge: "border-platform-generic-border bg-platform-generic-surface text-platform-generic-foreground",
    icon: "text-platform-generic-foreground",
    rateBadge: "bg-platform-generic-surface text-platform-generic-foreground",
  },
};
