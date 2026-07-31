export type DiscoveredModelUpdate = {
  modelAllowlist: string;
  preferredModels: string;
  defaultModelRemoved: boolean;
};

export function defaultModelFromPreferred(value: string) {
  return modelLines(value)[0] ?? "";
}

export function preferredModelsFromDefault(value: string) {
  const model = value.trim();
  return model ? model : "";
}

export function applyDiscoveredModels(
  models: string[],
  currentDefaultModel: string,
): DiscoveredModelUpdate {
  const normalizedModels = uniqueModels(models).sort((left, right) =>
    left.localeCompare(right, undefined, { numeric: true, sensitivity: "base" }),
  );
  const normalizedDefault = currentDefaultModel.trim();
  const defaultStillAvailable = normalizedDefault
    ? normalizedModels.some((model) => model.toLowerCase() === normalizedDefault.toLowerCase())
    : false;

  return {
    modelAllowlist: normalizedModels.join("\n"),
    preferredModels: defaultStillAvailable ? normalizedDefault : "",
    defaultModelRemoved: Boolean(normalizedDefault) && !defaultStillAvailable,
  };
}

export function addModelToList(value: string, model: string) {
  return uniqueModels([...modelLines(value), model]).join("\n");
}

export function removeModelFromList(value: string, model: string) {
  const normalized = model.trim().toLowerCase();
  return modelLines(value)
    .filter((item) => item.toLowerCase() !== normalized)
    .join("\n");
}

export function modelLines(value: string) {
  return uniqueModels(value.split(/\r?\n/));
}

function uniqueModels(models: string[]) {
  const seen = new Set<string>();
  return models.flatMap((model) => {
    const trimmed = model.trim();
    const normalized = trimmed.toLowerCase();
    if (!trimmed || seen.has(normalized)) {
      return [];
    }
    seen.add(normalized);
    return [trimmed];
  });
}
