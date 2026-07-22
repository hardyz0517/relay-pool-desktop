import { first } from "@/features/beta/first";
import("@/features/beta/second").then(({ second }) => second);
export const result = first;
