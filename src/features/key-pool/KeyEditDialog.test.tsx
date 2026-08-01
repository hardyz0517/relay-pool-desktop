// @vitest-environment jsdom
import { act, type FormEvent } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import { emptyEditForm } from "./KeyPoolFormModel";
import { KeyEditDialog } from "./KeyEditDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("KeyEditDialog", () => {
  it("delegates form changes and submit to page-owned handlers", async () => {
    const onFormChange = vi.fn();
    const onSave = vi.fn((event: FormEvent<HTMLFormElement>) => event.preventDefault());
    const host = document.createElement("div");
    const root = createRoot(host);
    const form = {
      ...emptyEditForm,
      stationId: "station-1",
      stationName: "Relay",
      name: "Primary",
    };

    await act(async () =>
      root.render(
        <KeyEditDialog
          actionSaving={false}
          form={form}
          groupOptions={[]}
          mode="create"
          sourceItem={null}
          stations={[{ id: "station-1", name: "Relay" } as Station]}
          onClose={vi.fn()}
          onFormChange={onFormChange}
          onSave={onSave}
          onStationChange={vi.fn()}
          renderCurrentGroupOption={() => []}
          renderGroupOptionLabel={(option: StationGroupOption) => option.groupName}
          renderGroupTriggerLabel={(option: StationGroupOption) => option.groupName}
        />,
      ),
    );

    const nameInput = document.body.querySelector<HTMLInputElement>("#key-pool-edit-form input")!;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    valueSetter.call(nameInput, "Primary Next");
    await act(async () => nameInput.dispatchEvent(new Event("input", { bubbles: true })));

    expect(onFormChange).toHaveBeenCalledWith({
      ...form,
      name: "Primary Next",
    });
    expect(document.body.querySelector('[aria-label="密钥状态"]')).toBeNull();

    const dialogForm = document.body.querySelector<HTMLFormElement>("#key-pool-edit-form")!;
    await act(async () => dialogForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));

    expect(onSave).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
  });
});
