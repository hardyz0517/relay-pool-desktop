export function createQueryErrorNotificationCycle() {
  const failedQueries = new Set<string>();

  return {
    shouldNotify(queryHash: string) {
      if (failedQueries.has(queryHash)) return false;
      failedQueries.add(queryHash);
      return true;
    },
    reset(queryHash: string) {
      failedQueries.delete(queryHash);
    },
  };
}
