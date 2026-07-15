import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function importTypeScriptModule(path) {
  const source = await readFile(path, "utf8");
  const match = source.match(
    /export function paginateRequestLogs[\s\S]*?\r?\n}\r?\n/,
  );
  assert.ok(match, "request log view model should export paginateRequestLogs");
  const transformed = match[0]
    .replace(/^export /, "")
    .replace(/: RequestLog\[\]/g, "")
    .replace(/: number/g, "");
  const encoded = Buffer.from(
    `${transformed}\nexport { paginateRequestLogs };`,
    "utf8",
  ).toString("base64");
  return import(`data:text/javascript;base64,${encoded}`);
}

const { paginateRequestLogs } = await importTypeScriptModule(
  "src/features/logs/requestLogViewModels.ts",
);

const logs = Array.from({ length: 45 }, (_, index) => ({ id: `log-${index + 1}` }));

const firstPage = paginateRequestLogs(logs, 1, 20);
assert.deepEqual(firstPage.logs.map((log) => log.id), logs.slice(0, 20).map((log) => log.id));
assert.deepEqual(
  { page: firstPage.page, totalPages: firstPage.totalPages, start: firstPage.startIndex, end: firstPage.endIndex },
  { page: 1, totalPages: 3, start: 1, end: 20 },
);

const lastPage = paginateRequestLogs(logs, 99, 20);
assert.deepEqual(lastPage.logs.map((log) => log.id), logs.slice(40).map((log) => log.id));
assert.deepEqual(
  { page: lastPage.page, totalPages: lastPage.totalPages, start: lastPage.startIndex, end: lastPage.endIndex },
  { page: 3, totalPages: 3, start: 41, end: 45 },
);

const emptyPage = paginateRequestLogs([], 4, 20);
assert.deepEqual(
  { page: emptyPage.page, totalPages: emptyPage.totalPages, start: emptyPage.startIndex, end: emptyPage.endIndex },
  { page: 1, totalPages: 1, start: 0, end: 0 },
);

const [pageSource, tableSource, dataTableSource] = await Promise.all([
  readFile("src/features/logs/LogsPage.tsx", "utf8"),
  readFile("src/features/logs/RequestLogTable.tsx", "utf8"),
  readFile("src/components/ui/DataTableLite.tsx", "utf8"),
]);

assert.ok(
  pageSource.includes("const [page, setPage] = useState(1)") &&
    pageSource.includes("const [pageSize, setPageSize] = useState(20)") &&
    pageSource.includes("paginateRequestLogs(filteredLogs, page, pageSize)"),
  "logs page should own pagination state and derive the visible request logs",
);

assert.ok(
  pageSource.includes("rows={pageInfo.logs}") &&
    pageSource.includes("<RequestLogPagination") &&
    pageSource.includes("pageInfo={pageInfo}"),
  "logs page should render only the current page and its pagination footer",
);

assert.ok(
  pageSource.includes("setPage(1)") &&
    pageSource.includes("handleFilterChange") &&
    pageSource.includes("handlePageSizeChange"),
  "filter, refresh, clear, and page-size changes should be able to reset pagination",
);

assert.ok(
  tableSource.includes("function RequestLogPagination") &&
    tableSource.includes("mt-4") &&
    tableSource.includes("第 {pageInfo.startIndex}-{pageInfo.endIndex} 条 / 共 {pageInfo.totalCount} 条"),
  "request log pagination should be separated from the records and show the visible range",
);

assert.ok(
  tableSource.includes("ChevronLeft") &&
    tableSource.includes("ChevronRight") &&
    tableSource.includes("每页") &&
    tableSource.includes("[20, 50, 100]") &&
    tableSource.includes('aria-label="上一页"') &&
    tableSource.includes('aria-label="下一页"'),
  "request log pagination should provide accessible page-size and chevron controls",
);

assert.ok(
  pageSource.includes('data-testid="request-log-toolbar-surface"') &&
    pageSource.includes('data-testid="request-log-table-surface"') &&
    tableSource.includes('data-testid="request-log-pagination-surface"'),
  "request log controls, records, and pagination should be separate surfaces",
);

assert.ok(
  pageSource.includes('data-testid="request-log-table-surface"') &&
    pageSource.includes('className="mt-3 overflow-hidden') &&
    tableSource.includes('data-testid="request-log-pagination-surface"') &&
    tableSource.includes('className="mt-4 flex'),
  "request log table should sit below the toolbar and pagination should remain below the table",
);

assert.ok(
  dataTableSource.includes('headerVariant?: "default" | "plain"') &&
    tableSource.includes('headerVariant="plain"'),
  "request logs should opt into the station-style plain table header without changing other tables",
);

assert.ok(
  dataTableSource.includes('headerVariant === "plain"') &&
    dataTableSource.includes("border-b border-border bg-surface") &&
    dataTableSource.includes("text-xs font-medium text-muted-foreground"),
  "plain table headers should use a white surface, normal spacing, and a bottom divider",
);
