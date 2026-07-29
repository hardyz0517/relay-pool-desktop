export type NativeUpdateLike = {
  currentVersion: string;
  version: string;
  body?: string;
};

export type PublishedUpdateInspection = {
  relation: "current_or_older" | "newer";
  version: string;
  notes: string | null;
};

type UpdateCheckDependencies<T extends NativeUpdateLike> = {
  currentVersion: string;
  proxyUrl: string | null;
  checkNative: (proxyUrl: string | null) => Promise<T | null>;
  inspectPublished: (currentVersion: string) => Promise<PublishedUpdateInspection>;
};

export class ManifestNewerButNativeUnavailableError extends Error {
  readonly code = "manifest-newer-native-unavailable";
  readonly publishedVersion: string;
  readonly nativeError: unknown;

  constructor(publishedVersion: string, nativeError: unknown) {
    super(`Published update ${publishedVersion} exists but the native updater is unavailable`);
    this.name = "ManifestNewerButNativeUnavailableError";
    this.publishedVersion = publishedVersion;
    this.nativeError = nativeError;
  }
}

export async function coordinateUpdateCheck<T extends NativeUpdateLike>(
  dependencies: UpdateCheckDependencies<T>,
) {
  try {
    const update = await dependencies.checkNative(dependencies.proxyUrl);
    return update
      ? { kind: "available" as const, update }
      : { kind: "current" as const, currentVersion: dependencies.currentVersion };
  } catch (nativeError) {
    let inspection: PublishedUpdateInspection;
    try {
      inspection = await dependencies.inspectPublished(dependencies.currentVersion);
    } catch {
      throw nativeError;
    }

    if (inspection.relation === "current_or_older") {
      return { kind: "current" as const, currentVersion: dependencies.currentVersion };
    }

    throw new ManifestNewerButNativeUnavailableError(inspection.version, nativeError);
  }
}
