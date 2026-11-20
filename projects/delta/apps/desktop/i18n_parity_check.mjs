import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const i18nDir = join(__dirname, "src", "packages", "i18n");

function extractKeys(filePath) {
  const source = readFileSync(filePath, "utf8");
  return new Set([...source.matchAll(/"([\w.]+)":/g)].map((m) => m[1]));
}

const en = extractKeys(join(i18nDir, "en.ts"));
const zh = extractKeys(join(i18nDir, "zh.ts"));

const missingInZh = [...en].filter((k) => !zh.has(k)).sort();
const missingInEn = [...zh].filter((k) => !en.has(k)).sort();

if (missingInZh.length === 0 && missingInEn.length === 0) {
  console.log("i18n parity: en.ts and zh.ts have the same keys");
  process.exit(0);
}

if (missingInZh.length > 0) {
  console.error(`Keys in en.ts missing from zh.ts (${missingInZh.length}):`);
  for (const k of missingInZh) console.error(`  ${k}`);
}
if (missingInEn.length > 0) {
  console.error(`Keys in zh.ts missing from en.ts (${missingInEn.length}):`);
  for (const k of missingInEn) console.error(`  ${k}`);
}
process.exit(1);
