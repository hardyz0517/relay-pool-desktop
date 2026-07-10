import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/pricing/ModelBasePricesPage.tsx", "utf8");
const editablePriceCellSource = source.slice(
  source.indexOf("function EditablePriceCell"),
  source.indexOf("function Field"),
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
