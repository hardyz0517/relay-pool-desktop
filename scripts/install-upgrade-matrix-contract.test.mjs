import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const scriptPath = resolve("scripts/run-install-upgrade-matrix.ps1");
const source = readFileSync(scriptPath, "utf8");

const mandatoryParams = ["OldInstaller", "NewInstaller", "OldVersion", "NewVersion", "OutputPath"];

for (const name of mandatoryParams) {
  const pattern = new RegExp(
    String.raw`\[Parameter\(Mandatory\s*=\s*\$true\)\]\s*\r?\n\s*\[ValidateNotNullOrEmpty\(\)\]\s*\r?\n\s*\[string\]\$${name}\b`,
  );
  if (!pattern.test(source)) {
    throw new Error(`${name} must be a mandatory non-empty parameter without a default value`);
  }

  const declaration = source.match(new RegExp(String.raw`\[string\]\$${name}[^\r\n]*`))?.[0] ?? "";
  if (declaration.includes("=")) {
    throw new Error(`${name} must not have a default value: ${declaration}`);
  }
}

for (const forbidden of [
  "0.3.2",
  "0.3.3",
  "Relay.Pool.Desktop_",
  "install-upgrade-matrix-v",
  "Downloads\\Relay.Pool.Desktop",
]) {
  if (source.includes(forbidden)) {
    throw new Error(`install/upgrade matrix script must not contain versioned defaults or labels: ${forbidden}`);
  }
}

for (const required of [
  "Install-Package \"fresh-install-candidate\" $newInstallerFull $NewVersion",
  "Install-Package \"install-supported-baseline\" $oldInstallerFull $OldVersion",
  "Install-Package \"upgrade-baseline-to-candidate\" $newInstallerFull $NewVersion",
  "version = $OldVersion",
  "version = $NewVersion",
  "installerSha256",
  "Resolve-ExplicitPath",
]) {
  if (!source.includes(required)) {
    throw new Error(`install/upgrade matrix script is missing explicit-parameter contract text: ${required}`);
  }
}

console.log("install/upgrade matrix explicit parameter contract ok");
