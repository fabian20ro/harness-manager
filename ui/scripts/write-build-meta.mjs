import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const buildId =
  process.env.VITE_BUILD_ID ||
  process.env.GITHUB_SHA ||
  `local-${new Date().toISOString()}`;
const builtAt = process.env.VITE_BUILD_TIME || new Date().toISOString();

const outPath = resolve(process.cwd(), "public", "build-meta.json");
await mkdir(resolve(process.cwd(), "public"), { recursive: true });
await writeFile(
  outPath,
  JSON.stringify(
    {
      buildId,
      builtAt,
      generatedBy: "ui/scripts/write-build-meta.mjs",
    },
    null,
    2,
  ),
);
