export const DEFAULT_MANUAL_PROXY_URL = "http://127.0.0.1:7890";

export function withManualProxyDefault<T extends { collectorProxyUrl: string }>(
  form: T,
): T {
  const currentUrl = form.collectorProxyUrl.trim();
  return {
    ...form,
    collectorProxyUrl: currentUrl || DEFAULT_MANUAL_PROXY_URL,
  };
}
