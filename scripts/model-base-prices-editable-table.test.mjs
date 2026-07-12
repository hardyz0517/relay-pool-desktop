import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/pricing/ModelBasePricesPage.tsx", "utf8");
const editablePriceCellSource = source.slice(
  source.indexOf("function EditablePriceCell"),
  source.indexOf("function Field"),
);
const createDialogSource = source.slice(source.indexOf("<Dialog"), source.indexOf("function TableColumnHeaderRow"));
const dateFieldSource = source.slice(
  source.indexOf("function DateField"),
  source.indexOf("function DatePickerPanel"),
);

assert.ok(
  source.includes("providerFilterOptions"),
  "model base prices page should expose provider filters for scanning by vendor",
);

assert.ok(
  source.includes("EditablePriceCell"),
  "input and output prices should be editable directly from table cells",
);

assert.ok(
  source.includes("numberText = formatPrice(value)") && source.includes("{saving ? \"保存中\" : numberText}") && source.includes("$/M"),
  "editable price cells should render compact prices with the currency/unit outside the editable number",
);

assert.ok(
  !source.includes("formatPriceUnit") && !source.includes("formatPriceAmount") && !source.includes("`${currency} · ${unit}`"),
  "editable price cells should not expose raw English currency/unit fields",
);

assert.ok(
  !source.includes("min-w-[164px]") && !source.includes("w-16") && source.includes("style={{ width: numberBoxWidth }}"),
  "editable price cells should use matching compact widths instead of fixed widths that create padding or click growth",
);

assert.ok(
  editablePriceCellSource.includes('inputMode="decimal"') &&
    editablePriceCellSource.includes('pattern="[0-9]*[.]?[0-9]*"') &&
    editablePriceCellSource.includes('type="text"') &&
    !editablePriceCellSource.includes('type="number"'),
  "editable price cells should use a decimal text input so native number steppers do not hide digits or cause jump",
);

assert.ok(
  editablePriceCellSource.includes("text-center") &&
    editablePriceCellSource.includes("justify-center") &&
    !editablePriceCellSource.includes("px-1 text-right text-sm") &&
    !editablePriceCellSource.includes("justify-end rounded-[7px] px-1 text-right"),
  "editable price boxes should center the numeric text inside the hover/edit frame",
);

assert.ok(
  source.includes("commitEdit"),
  "editable price cells should save automatically on commit",
);

assert.ok(
  source.includes("<Dialog") && source.includes("新增基准价格"),
  "new model base prices should be created from a dialog",
);

assert.ok(
  !source.includes('title={draft.id ? "编辑基准价格" : "新增基准价格"}'),
  "model base prices page should not keep the bottom edit/create form",
);

assert.ok(
  !/<th[^>]*>\s*单位\s*<\/th>/.test(source),
  "unit should not be a standalone table column",
);

assert.ok(
  !/<th[^>]*>\s*检查日期\s*<\/th>/.test(source),
  "checked date should not be a standalone table column",
);

assert.ok(
  !/<th[^>]*>\s*来源\s*<\/th>/.test(source),
  "source should not be a standalone table column",
);

assert.ok(
  source.includes("<thead>") && !source.includes("bg-teal-50/70"),
  "table column headers should use neutral per-group headers instead of a global teal header band",
);

assert.ok(
  source.includes('className="grid gap-3 px-4 py-4"') &&
    source.includes("<TableColumnHeaderRow />") &&
    /group\.label[\s\S]*<TableColumnHeaderRow \/>/.test(source),
  "each provider group title should appear before its own column header row",
);

assert.ok(
  source.includes("showLabel={false}"),
  "status switches should render as bare toggles without the extra enabled/disabled pill label",
);

assert.ok(
  source.includes("currencyOptions") &&
    createDialogSource.includes('<SelectField label="币种"') &&
    createDialogSource.includes("options={currencyOptions}"),
  "new base price dialog should use a dropdown for currency instead of a free text input",
);

assert.ok(
  source.includes("unitOptions") &&
    createDialogSource.includes('<SelectField label="单位"') &&
    createDialogSource.includes("options={unitOptions}") &&
    source.includes('{ value: "M", label: "M" }') &&
    !source.includes('unit: "per_1m_tokens"') &&
    !source.includes('|| "per_1m_tokens"'),
  "new base price dialog should use short unit choices such as K/M/B instead of per_1m_tokens",
);

assert.ok(
  source.includes("function createEmptyDraft") &&
    source.includes("formatLocalDate(new Date())") &&
    source.includes("setCreateDraft(createEmptyDraft())"),
  "new base price dialog should default checked date to the current local computer date each time it opens",
);

assert.ok(
  createDialogSource.includes('<DateField label="检查日期"') &&
    source.includes("CalendarDays") &&
    source.includes("function DateField") &&
    source.includes("function DatePickerPanel") &&
    source.includes("createPortal(") &&
    source.includes('className="fixed z-[70] w-[236px]') &&
    !createDialogSource.includes('inputType="date"') &&
    !source.includes('type={numeric ? "number" : inputType ?? "text"}'),
  "checked date should use a compact app-styled date picker instead of the browser-native date input",
);

assert.ok(
  source.includes(
    'import { useInteractionActivity } from "@/components/ui/InteractionActivity";',
  ) && dateFieldSource.includes("const interactionActive = useInteractionActivity();"),
  "the date field should consume interaction activity before rendering its body portal",
);

assert.match(
  dateFieldSource,
  /useLayoutEffect\(\(\) => \{\s*if \(interactionActive\) \{\s*return;\s*\}\s*setOpen\(false\);\s*setPosition\(null\);\s*\}, \[interactionActive\]\);/,
  "the date field should clear open state and portal geometry during the inactive commit",
);

assert.ok(
  dateFieldSource.includes("interactionActive && open && position"),
  "the date picker portal should be omitted from the current inactive render",
);

assert.ok(
    source.includes("const canOpenBelow = spaceBelow >= panelHeight;") &&
    source.includes("const canOpenAbove = spaceAbove >= panelHeight;") &&
    source.includes("const openBelow = canOpenBelow || !canOpenAbove;") &&
    source.includes("const preferredLeft = rect.right - panelWidth;") &&
    !source.includes("spaceAbove > spaceBelow"),
  "date picker should prefer opening below whenever there is enough viewport space and align to the trigger instead of drifting left",
);

assert.ok(
  !createDialogSource.includes("启用模型基准价格") &&
    !createDialogSource.includes("onCheckedChange={() => setCreateDraft({ ...createDraft, enabled: !createDraft.enabled })}"),
  "new base price dialog should omit the enabled switch while keeping new rows enabled by default",
);
