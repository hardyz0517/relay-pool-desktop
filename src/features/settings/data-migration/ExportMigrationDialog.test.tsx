// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { ExportMigrationDialog } from "./ExportMigrationDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("ExportMigrationDialog", () => {
  it("shows the minimum passphrase length before input", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <ExportMigrationDialog capability={null} busy={false} open onClose={vi.fn()} onSubmit={vi.fn()} />,
    ));

    const placeholders = [...document.querySelectorAll<HTMLInputElement>("input[placeholder]")]
      .map((input) => input.placeholder);

    expect(placeholders).toContain("至少输入 12 个字符");
    expect(placeholders).toContain("再次输入迁移密码");

    await act(async () => root.unmount());
  });
});
