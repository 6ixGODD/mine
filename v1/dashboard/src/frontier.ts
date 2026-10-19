import Ajv2020 from "ajv/dist/2020.js";
import schema from "../../contracts/schema.json" with { type: "json" };
import type { FrontierSnapshot } from "./generated/contracts";

const validator = new Ajv2020({
  strict: true,
  allErrors: true,
  coerceTypes: false,
  removeAdditional: false,
  useDefaults: false,
});
const validate = validator.compile<FrontierSnapshot>({
  $schema: schema.$schema,
  $defs: schema.$defs,
  $ref: "#/$defs/FrontierSnapshot",
});

/** Validates the shared wire boundary before any view mapping; no field coercion. */
export function parseFrontier(value: unknown): FrontierSnapshot {
  if (!validate(value))
    throw new Error(
      `Invalid FrontierSnapshot: ${validator.errorsText(validate.errors)}`,
    );
  // JSON Schema preserves the canonical decimal string; Rust also caps it to SQLite signed64.
  // BigInt is mandatory here: Number would round valid cursors above 2^53.
  if (BigInt(value.cursor) > 9223372036854775807n)
    throw new Error(
      "Invalid FrontierSnapshot: cursor exceeds SQLite signed64 range",
    );
  return value;
}
