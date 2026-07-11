const UPDATE_SOURCE_UNREACHABLE =
  "\u68c0\u67e5\u66f4\u65b0\u5931\u8d25\uff1a\u65e0\u6cd5\u8fde\u63a5\u5230 GitHub \u66f4\u65b0\u6e90\uff0c\u8bf7\u68c0\u67e5\u7f51\u7edc\u6216\u7a0d\u540e\u91cd\u8bd5\u3002";
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
