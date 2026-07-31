import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const oldFinalizationFile = resolve("src-tauri/src/application/request_finalization.rs");
const finalizationMod = resolve("src-tauri/src/application/request_finalization/mod.rs");
const failureFile = resolve("src-tauri/src/application/request_finalization/failure.rs");
const effectFile = resolve("src-tauri/src/application/request_finalization/effect_planner.rs");
const routingFailureFile = resolve("src-tauri/src/application/routing_engine/routing_failure.rs");
const proxyErrorFile = resolve("src-tauri/src/services/proxy/error.rs");

if (existsSync(oldFinalizationFile)) {
  throw new Error("request_finalization.rs must be atomically converted to request_finalization/mod.rs");
}
for (const file of [finalizationMod, failureFile, effectFile]) {
  if (!existsSync(file)) throw new Error(`Missing request finalization module artifact: ${file}`);
}

const failure = readFileSync(failureFile, "utf8");
for (const text of [
  "pub(crate) enum FailureTarget",
  "ModelOnKey",
  "StationKeyCredential",
  "StationAccount",
  "StationEndpoint",
  "ProviderProtocol",
  "LocalAdapter",
  "Downstream",
  "Uncertain",
  "pub(crate) enum FailureClass",
  "pub(crate) enum RetryDisposition",
  "pub(crate) enum HealthEffect",
  "pub(crate) enum CapabilityEffect",
  "pub(crate) fn public_error_for_class",
  "ProviderErrorSemanticSignal::GenericStatus",
]) {
  if (!failure.includes(text)) throw new Error(`Missing canonical failure contract text: ${text}`);
}
for (const forbidden of ["_ => PublicError", "Unknown=500", "unknown=500"]) {
  if (failure.includes(forbidden)) throw new Error(`Forbidden non-exhaustive public mapping text: ${forbidden}`);
}

const routingFailure = readFileSync(routingFailureFile, "utf8");
for (const text of [
  "Some(403 | 404) => ClassifiedRouteFailure::request_only(RouteFailureKind::Uncertain, false)",
  "pub(crate) enum RoutePlanningFailure",
  "fn generic_forbidden_is_uncertain_and_neutral",
  "adapter_confirmed_model_not_found_can_update_capability_only_when_applicable",
]) {
  if (!routingFailure.includes(text)) throw new Error(`Missing routing failure typed contract text: ${text}`);
}
if (/Some\(401\s*\|\s*403\)/u.test(routingFailure) || /RouteFailureKind::ModelUnavailable[\s\S]*RouteFailureInput::http_status\(404/u.test(routingFailure)) {
  throw new Error("Generic 403/404 must not be classified as auth/model failure");
}

const proxyError = readFileSync(proxyErrorFile, "utf8");
for (const text of [
  "RouteConfigRequired",
  "routing_configuration_required",
  "RouteEconomicsUnavailable",
  "RouteHealthUnavailable",
  "RouteCapacityExhausted",
  "RouteInvariantViolation",
  "UpstreamAuthenticationFailed",
  "UpstreamModelUnavailable",
  "UpstreamUncertain",
  "pub(crate) fn from_public_error",
]) {
  if (!proxyError.includes(text)) throw new Error(`Missing proxy error mapping contract text: ${text}`);
}
for (const source of [failure, routingFailure, proxyError]) {
  if (source.includes('"route_config_required"')) {
    throw new Error("routing configuration admission must use routing_configuration_required, not the legacy route_config_required code");
  }
}

console.log("routing error contract ok");
