import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const app = await readFile("src/app/App.tsx", "utf8");
const host = await readFile("src/app/ShellPageHost.tsx", "utf8");
const styles = await readFile("src/styles.css", "utf8");

assert.match(app, /useNavigationController\("dashboard"\)/);
assert.match(app, /activeRouteId=\{intent\.shellRouteId\}/);
assert.match(host, /state=\{shellPageState === "entering" \? "entering" : shellPageState\}/);
assert.match(host, /event\.target === event\.currentTarget/);
assert.match(styles, /data-page-transition-state="entering"/);
assert.ok(!host.includes('mode="wait"'));
assert.ok(styles.includes("opacity"));
assert.ok(!/entering[\s\S]{0,300}(translate|scale|blur)/.test(styles));

console.log("navigation handoff contract passed");
