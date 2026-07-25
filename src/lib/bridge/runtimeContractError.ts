export class RuntimeContractMismatchError extends Error {
  readonly reason: string;

  constructor(reason: string) {
    super(`Desktop runtime contract mismatch: ${reason}`);
    this.name = "RuntimeContractMismatchError";
    this.reason = reason;
  }
}

export function isRuntimeContractMismatch(error: unknown): error is RuntimeContractMismatchError {
  return error instanceof RuntimeContractMismatchError
    || (error instanceof Error && error.name === "RuntimeContractMismatchError");
}
