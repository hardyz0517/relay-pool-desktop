const UPDATE_SOURCE_UNREACHABLE =
  "\u68c0\u67e5\u66f4\u65b0\u672a\u5b8c\u6210\uff1a\u65e0\u6cd5\u8bfb\u53d6 GitHub \u66f4\u65b0\u6e90\uff1b\u5982\u679c\u66f4\u65b0\u65e5\u5fd7\u4e0e\u5f53\u524d\u7248\u672c\u4e00\u81f4\uff0c\u8bf4\u660e\u5df2\u662f\u6700\u65b0\u7248\u672c\u3002";
const UPDATE_FILE_NOT_PUBLISHED =
  "\u68c0\u67e5\u66f4\u65b0\u5931\u8d25\uff1a\u5f53\u524d\u8fd8\u6ca1\u6709\u53d1\u5e03\u53ef\u7528\u7684\u66f4\u65b0\u6587\u4ef6\u3002";
const UPDATE_SIGNATURE_INVALID =
  "\u68c0\u67e5\u66f4\u65b0\u5931\u8d25\uff1a\u66f4\u65b0\u7b7e\u540d\u6821\u9a8c\u672a\u901a\u8fc7\u3002";

export function normalizeUpdaterError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);

  if (/error sending request|timed out|timeout|network|dns|resolve|connection|certificate|tls/i.test(message)) {
    return UPDATE_SOURCE_UNREACHABLE;
  }

  if (/404|not found/i.test(message) && /latest\.json/i.test(message)) {
    return UPDATE_FILE_NOT_PUBLISHED;
  }

  if (/signature|pubkey|verify/i.test(message)) {
    return UPDATE_SIGNATURE_INVALID;
  }

  return message;
}
