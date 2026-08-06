import type { StationType } from "@/lib/types/stations";

export type ProviderPresetId = "newapi" | "sub2api";

export type ProviderPreset = {
  id: ProviderPresetId;
  name: string;
  description: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
};

export const providerPresets: ProviderPreset[] = [
  {
    id: "sub2api",
    name: "Sub2API",
    description: "Sub2API station with balance and group collection support.",
    stationType: "sub2api",
    websiteUrl: "",
    apiBaseUrl: "",
  },
  {
    id: "newapi",
    name: "NewAPI",
    description: "NewAPI station with management collection support.",
    stationType: "newapi",
    websiteUrl: "",
    apiBaseUrl: "",
  },
];
