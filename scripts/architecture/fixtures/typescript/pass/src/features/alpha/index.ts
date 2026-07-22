import { value } from "@/features/beta";
import type { SharedName } from "./local";
import { SharedName as RuntimeName } from "./local";
export const result: SharedName = value + RuntimeName;
