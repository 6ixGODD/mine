import { registerHooks } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

const environment = process.env.MINE_UI_DEPENDENCIES;
if (!environment || !path.isAbsolute(environment))
  throw new Error("Root-owned dependency location is required.");
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (
      specifier.startsWith(".") ||
      specifier.startsWith("/") ||
      specifier.includes(":")
    )
      return nextResolve(specifier, context);
    return nextResolve(specifier, {
      ...context,
      parentURL: pathToFileURL(path.join(environment, "resolver.mjs")).href,
    });
  },
});
