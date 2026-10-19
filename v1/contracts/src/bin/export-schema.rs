//! Deterministic schema export; --check is read-only.
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 1 || !matches!(args[0].as_str(), "--check" | "--write") {
        return Err("usage: export-schema --check|--write".into());
    }
    let schema = schemars::schema_for!(mine_v1_contracts::ContractSchema);
    let expected = format!("{}\n", serde_json::to_string_pretty(&schema)?);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema.json");
    if args[0] == "--write" {
        fs::write(path, expected)?;
    } else if fs::read_to_string(path)? != expected {
        return Err("contract schema drift; regenerate and review consumers".into());
    }
    Ok(())
}
