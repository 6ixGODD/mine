import {
  readFileSync,
  mkdirSync,
  copyFileSync,
  existsSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";
import { resolvePaths } from "./paths.mjs";

const dashboard = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const { output, environment, isRoot } = resolvePaths(dashboard);
const modules = path.join(environment, "node_modules");
const action = process.argv[2];

if (action === "prepare") {
  if (!isRoot)
    throw new Error(
      "Only the Root setup owner may refresh shared dependencies.",
    );
  // Root-owned setup is serialized; callers must stop all UI writers before refreshing it.
  mkdirSync(environment, { recursive: true });
  for (const name of ["package.json", "package-lock.json"]) {
    copyFileSync(path.join(dashboard, name), path.join(environment, name));
  }
  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  const result = spawnSync(
    npm,
    ["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
    {
      cwd: environment,
      stdio: "inherit",
      windowsHide: true,
      shell: process.platform === "win32",
    },
  );
  process.exit(result.status ?? 1);
}
if (
  ![
    "build",
    "dev",
    "typecheck",
    "test",
    "generate-contract",
    "check-contract",
  ].includes(action)
) {
  throw new Error(
    "Expected prepare, build, dev, typecheck, test, generate-contract or check-contract.",
  );
}
for (const name of ["package.json", "package-lock.json"]) {
  const installed = path.join(environment, name);
  if (
    !existsSync(installed) ||
    !readFileSync(installed).equals(readFileSync(path.join(dashboard, name)))
  ) {
    throw new Error(
      "Root UI dependency environment is absent or differs; run npm run prepare-env with exclusive setup ownership.",
    );
  }
}
const schema = JSON.parse(
  readFileSync(path.resolve(dashboard, "../contracts/schema.json"), "utf8"),
);
const { compile } = await import(
  pathToFileURL(
    path.join(modules, "json-schema-to-typescript", "dist", "src", "index.js"),
  ).href
);
const generated = await compile(schema, "ContractSchema", {
  bannerComment:
    "/* Generated from ../contracts/schema.json by json-schema-to-typescript. Do not edit. */",
  unreachableDefinitions: true,
});
const generatedPath = path.join(
  dashboard,
  "src",
  "generated",
  "contracts.d.ts",
);
if (action === "generate-contract") {
  mkdirSync(path.dirname(generatedPath), { recursive: true });
  writeFileSync(generatedPath, generated);
  console.log("Generated UI types from the shared Rust-produced schema.");
  process.exit(0);
}
if (
  !existsSync(generatedPath) ||
  readFileSync(generatedPath, "utf8") !== generated
) {
  throw new Error(
    "Shared schema and generated UI types differ; the contract owner must regenerate and verify.",
  );
}
if (action === "check-contract") {
  console.log("Shared schema/type parity passed.");
  process.exit(0);
}
if (action === "test") {
  const result = spawnSync(
    process.execPath,
    [
      "--import",
      pathToFileURL(path.join(dashboard, "scripts", "resolve-env.mjs")).href,
      "--test",
      "tests/model.test.ts",
      "tests/contract.test.ts",
      "tests/paths.test.mjs",
    ],
    {
      cwd: dashboard,
      stdio: "inherit",
      windowsHide: true,
      env: { ...process.env, MINE_UI_DEPENDENCIES: environment },
    },
  );
  process.exit(result.status ?? 1);
}
if (action === "typecheck") {
  const ts = (
    await import(
      pathToFileURL(path.join(modules, "typescript", "lib", "typescript.js"))
        .href
    )
  ).default;
  const configPath = path.join(dashboard, "tsconfig.json");
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    dashboard,
  );
  parsed.options.typeRoots = [path.join(modules, "@types")];
  const host = ts.createCompilerHost(parsed.options);
  host.resolveModuleNames = (names, containingFile) =>
    names.map((name) => {
      const from = name.startsWith(".")
        ? containingFile
        : path.join(environment, "resolver.ts");
      return ts.resolveModuleName(name, from, parsed.options, ts.sys)
        .resolvedModule;
    });
  const program = ts.createProgram(parsed.fileNames, parsed.options, host);
  const diagnostics = [
    ...(config.error ? [config.error] : []),
    ...parsed.errors,
    ...ts.getPreEmitDiagnostics(program),
  ];
  if (diagnostics.length) {
    process.stderr.write(
      ts.formatDiagnosticsWithColorAndContext(diagnostics, {
        getCanonicalFileName: (name) => name,
        getCurrentDirectory: () => dashboard,
        getNewLine: () => "\n",
      }),
    );
    process.exit(1);
  }
  console.log("Dashboard typecheck passed (isolated prototype only).");
} else {
  const vite = await import(
    pathToFileURL(path.join(modules, "vite", "dist", "node", "index.js")).href
  );
  const tailwind = (
    await import(
      pathToFileURL(
        path.join(modules, "@tailwindcss", "vite", "dist", "index.mjs"),
      ).href
    )
  ).default;
  const config = {
    root: dashboard,
    configFile: false,
    cacheDir: path.join(output, "vite-cache"),
    plugins: [tailwind()],
    resolve: {
      alias: {
        "@": path.join(dashboard, "src"),
        ajv: path.join(modules, "ajv"),
        "@base-ui/react": path.join(modules, "@base-ui", "react"),
        "@fontsource-variable/geist": path.join(
          modules,
          "@fontsource-variable",
          "geist",
        ),
        "class-variance-authority": path.join(
          modules,
          "class-variance-authority",
        ),
        cn: path.join(modules, "cn"),
        "lucide-react": path.join(modules, "lucide-react"),
        tailwindcss: path.join(modules, "tailwindcss"),
        "tw-animate-css": path.join(modules, "tw-animate-css"),
        "shadcn/tailwind.css": path.join(
          modules,
          "shadcn",
          "dist",
          "tailwind.css",
        ),
        shadcn: path.join(modules, "shadcn"),
        "react-dom": path.join(modules, "react-dom"),
        react: path.join(modules, "react"),
      },
    },
    esbuild: { jsx: "automatic" },
    build: { outDir: path.join(output, "dist"), emptyOutDir: false },
    server: {
      host: "127.0.0.1",
      port: 4178,
      strictPort: true,
      fs: {
        allow: [dashboard, path.resolve(dashboard, "../contracts"), modules],
      },
    },
  };
  if (action === "build") await vite.build(config);
  else {
    const server = await vite.createServer(config);
    await server.listen();
    server.printUrls();
    console.log("DEMO ONLY: fixtures, no backend or session control.");
  }
}
