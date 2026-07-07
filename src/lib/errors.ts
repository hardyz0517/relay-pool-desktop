export function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
