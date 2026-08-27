import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  InspectPortableImportInput,
  PortableExportResult,
  PortableImportRecoveryState,
  PortableMigrationCapability,
  PortableMigrationOperation,
  PortableMigrationOperationStarted,
  PortablePathToken,
  PreparePortableImportInput,
  StartPortableExportInput,
} from "@/lib/types/dataMigration";

export function getPortableMigrationCapability(): Promise<PortableMigrationCapability> {
  return getActiveBackendClient().dataMigration.getPortableMigrationCapability();
}

export function choosePortableExportPath(): Promise<PortablePathToken | null> {
  return getActiveBackendClient().dataMigration.choosePortableExportPath();
}

export function startPortableExport(input: StartPortableExportInput): Promise<PortableMigrationOperationStarted> {
  return getActiveBackendClient().dataMigration.startPortableExport(input);
}

export function getPortableExportResult(resourceId: string): Promise<PortableExportResult> {
  return getActiveBackendClient().dataMigration.getPortableExportResult(resourceId);
}

export function choosePortableImportFile(): Promise<PortablePathToken | null> {
  return getActiveBackendClient().dataMigration.choosePortableImportFile();
}

export function startPortableImportInspection(
  input: InspectPortableImportInput,
): Promise<PortableMigrationOperationStarted> {
  return getActiveBackendClient().dataMigration.startPortableImportInspection(input);
}

export function startPortableImportPrepare(
  input: PreparePortableImportInput,
): Promise<PortableMigrationOperationStarted> {
  return getActiveBackendClient().dataMigration.startPortableImportPrepare(input);
}

export function getPortableMigrationOperation(operationId: string): Promise<PortableMigrationOperation> {
  return getActiveBackendClient().dataMigration.getPortableMigrationOperation(operationId);
}

export function getPortableImportRecoveryState(): Promise<PortableImportRecoveryState> {
  return getActiveBackendClient().dataMigration.getPortableImportRecoveryState();
}
