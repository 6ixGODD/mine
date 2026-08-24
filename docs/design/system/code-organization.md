# Code Organization

## Initial package strategy

v1 uses one Rust package. Split crates only after stable boundaries and a real publication or compile-time need exist.

```text
src/
├── lib.rs
├── main.rs
├── cli/
├── mcp/
├── domain/
│   ├── config.rs
│   ├── error.rs
│   ├── ports.rs
│   ├── graph.rs
│   ├── adr.rs
│   ├── plan_id.rs
│   ├── plan_release.rs
│   ├── rewire.rs
│   ├── status.rs
│   ├── path.rs
│   ├── design_marker.rs
│   ├── design_reference.rs
│   ├── repository_identity.rs
│   └── validation.rs
├── application/
│   ├── init_service.rs
│   ├── workspace_service.rs
│   ├── graph_service.rs
│   ├── plan_service.rs
│   ├── adr_service.rs
│   ├── design_service.rs
│   ├── agent_service.rs
│   ├── doctor_service.rs
│   ├── release_service.rs
│   └── scan_service.rs
├── infrastructure/
│   ├── toml_store.rs
│   ├── adr_store.rs
│   ├── atomic_write.rs
│   ├── file_lock.rs
│   ├── git.rs
│   ├── design_backup.rs
│   ├── embedded_skills.rs
│   └── system.rs
├── render/
├── setup/
├── agent_setup/
│   ├── backup.rs
│   ├── config_edit.rs
│   ├── doctor.rs
│   ├── install.rs
│   ├── managed_state.rs
│   ├── safety.rs
│   ├── targets.rs
│   ├── transaction.rs
│   └── uninstall.rs
└── output/
```

## Ownership rules

- Domain modules contain pure rules and typed errors.
- Application modules define use cases and ports.
- Infrastructure performs I/O.
- Agent configuration formats and target locations stay behind the shared
  `agent_setup` adapters and typed `Agent` selection.
- Root `skills/` is the only manually maintained Skill source.
- No module parses both CLI arguments and domain state.

## Configuration source rules

Cargo, rustfmt, clippy policy, tests, and release workflow each have one authoritative configuration source.
