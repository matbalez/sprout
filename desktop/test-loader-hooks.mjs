import { fileURLToPath } from "node:url";
import path from "node:path";

const srcRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "src",
);

export function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith("@/")) {
    const sourcePath = specifier.slice(2);
    const extension = path.extname(sourcePath) ? "" : ".ts";
    const resolved = `${srcRoot}/${sourcePath}${extension}`;
    return nextResolve(resolved, context);
  }
  // Resolve extensionless relative TS imports (e.g. `./parseImeta`) — the app's
  // bundler adds the extension, but node's ESM resolver does not. Without this,
  // any .ts that relative-imports a sibling .ts can't be imported from a test,
  // which previously forced stale inlined copies of the source under test.
  if (
    (specifier.startsWith("./") || specifier.startsWith("../")) &&
    !path.extname(specifier) &&
    context.parentURL
  ) {
    const resolved = new URL(`${specifier}.ts`, context.parentURL).href;
    return nextResolve(resolved, context);
  }
  return nextResolve(specifier, context);
}
