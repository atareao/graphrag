# AGENT DIRECTIVES: OPENSPEC (SDD) + TDD WORKFLOW

## ⚠️ REGLA DE ORO — LEER ANTES DE ACTUAR

**ANTES de escribir o editar CUALQUIER archivo de código fuente (Rust, TypeScript, JSX, CSS, etc.),
debes ejecutar `just check-spec` para confirmar que existe un change proposal aprobado.**

Si `just check-spec` falla:
1. DETENTE inmediatamente.
2. Informa al usuario que no hay un change proposal activo.
3. Pregunta si quiere crear uno con `openspec new change <feature>`.
4. NO escribas código hasta recibir aprobación explícita.

**SALTARSE ESTE PASO ES VIOLACIÓN DEL PROTOCOLO.**

---

## I. CORE PRINCIPLES & GOALS

- **Phase 0 — Legacy Support:** If modifying existing code without specs or tests, establish a baseline spec and characterization tests before introducing changes.
- **Phase 1 — SDD (OpenSpec):** No new code or tests may be written before a spec change proposal exists in `openspec/changes/<feature>/` and is approved by the user.
- **Phase 2 — TDD (Red-Green-Refactor):** Once the spec is approved, code MUST be developed strictly test-first using terminal commands.
- **Strict Verification:** Always run CLI test suites using terminal tools. Never assume code or tests pass/fail without CLI confirmation.

---

## II. EXECUTION WORKFLOW

### Phase 0: Legacy Code Preparation (Conditional)

*Execute this phase ONLY if modifying an existing module/file that lacks OpenSpec documentation or tests.*

1. **Characterization Spec (As-Is):**
   - Inspect the target file/module.
   - Generate a baseline spec in `openspec/specs/<module>/spec.md` reflecting current behavior.
2. **Characterization Tests:**
   - Write Rust (`#[test]`) or React/TS (`vitest` / `@testing-library/react`) tests matching current behavior.
   - Run tests via CLI (`cargo test` or `npx vitest run`) to confirm all pass in **GREEN**.

### Phase 1: SDD Protocol (OpenSpec)

When the user requests a new feature, bug fix, or refactor:

1. **Create the Change Proposal:**
   - Execute CLI command: `openspec new change <feature-name>`
2. **Draft Specifications:**
   - Populate `openspec/changes/<feature-name>/proposal.md` with intent, scope, and impact.
   - Create spec deltas in `openspec/changes/<feature-name>/specs/<module>/spec.md`.
   - Ensure the spec includes:
     - **Contracts:** Rust types/structs/enums, TypeScript interfaces/props, API endpoints, or function signatures.
     - **Scenarios (BDD style):** Detailed `Given / When / Then` clauses for happy path, error cases, and edge cases.
   - Populate `openspec/changes/<feature-name>/tasks.md` with the TDD task checklist.
3. **STOP & WAIT FOR APPROVAL:**
   - Present the created specification to the user.
   - **DO NOT** write application code or new tests until the user explicitly approves the spec.

### Phase 2: TDD Protocol (Red-Green-Refactor)

Once the user approves the spec (e.g., "Approved", "Looks good", "Proceed with TDD"):

1. **RED (Write Failing Tests):**
   - Read the `Given / When / Then` scenarios in `openspec/changes/<feature-name>/specs/`.
   - Write tests in Rust or React/TypeScript corresponding to those scenarios.
   - Execute CLI tests (`cargo test` or `npx vitest run`).
   - **Verify:** Confirm test failure for the new functionality while any legacy tests remain **GREEN**.
2. **GREEN (Minimal Implementation):**
   - Write the absolute minimum code necessary to satisfy the failing tests.
   - Execute CLI tests (`cargo test` or `npx vitest run`).
   - Run type checks (`cargo check` or `npx tsc --noEmit`).
   - **Verify:** Confirm all tests pass (100% green) and no compilation/type errors exist.
3. **REFACTOR (Clean & Consolidate):**
   - Clean up code formatting, types, and structure without altering behavior.
   - Run linters (`cargo clippy -- -D warnings` / `npm run lint`).
   - Re-run test suites via CLI to guarantee no regressions.
4. **CONSOLIDATE & ARCHIVE:**
   - Mark completed items in `tasks.md`.
   - Once all scenarios pass, run `openspec archive <feature-name>` to merge the delta into `openspec/specs/`.

### Practical Lessons Learned (SDD + TDD)

#### Archive requires exact header matching
`openspec archive` busca el header exacto del delta en la spec destino. Si el header del delta es `"### Requirement: Pipeline evaluation order (WAF first)"` pero la spec tiene `"### Requirement: Pipeline evaluation order"`, el archive falla. **Los headers del delta deben copiar EXACTAMENTE los de la spec destino.**

#### Si reescribes la spec directamente, no intentes archivar
Si modificaste `openspec/specs/<module>/spec.md` a mano (fuera del mecanismo de archive), el change proposal correspondiente queda huérfano. No se puede archivar porque los headers ya no coinciden. **Solución: eliminar el directorio del change proposal** (`rm -rf openspec/changes/<feature>/`).

#### Cambios en cascada
Eliminar una entidad (ej. Whitelist/Blacklist) puede dejar código muerto en otras partes (ej. `AppError::Conflict`, tests de Conflict). El REFACTOR phase debe incluir la limpieza de estos artefactos. **Siempre ejecutar `cargo clippy -- -D warnings` tras el GREEN phase para detectar código/ variantes no usados.**

#### `openspec archive --yes` no bypassa validación de headers
La flag `--yes` salta la comprobación de tareas incompletas, pero NO la validación de que los headers del delta existan en la spec destino. Si los headers no matchean, el archive igual falla.

#### Mantén openspec artifacts sincronizados con el código
Si implementas un cambio en código pero no actualizas los artifacts de openspec (tasks, proposal), el change proposal queda "stuck" — no se puede archivar ni continuar. **Antes de empezar un nuevo cambio, verifica que no haya cambios activos huerfanos con `openspec list`.**

---

## III. PROJECT CONFIGURATION & CONVENTIONS

### Stack Commands

#### Backend: Rust
- **Test Runner:** `cargo test` (or `cargo nextest run` if available).
- **Type Checking & Linting:** `cargo check` and `cargo clippy -- -D warnings` (enforce zero warnings).
- **Formatting:** `cargo fmt --check`
- **Conventions:**
  - Structs and types placed in domain modules or `src/models/`.
  - Unit tests placed in the same file under `#[cfg(test)]`.
  - Integration and API tests placed in `tests/`.

### Custom Repository Rules

- Insert here any specific business logic, database conventions, or custom architectural rules unique to this project.

---

## IV. RESPONSE FORMAT & STATUS MESSAGES

Always prefix your progress updates with the current status tag:

```text
[LEGACY - INSPECT] Creating baseline spec & characterization tests.
[OPENSPEC - DRAFT] Generating change proposal in openspec/changes/...
[OPENSPEC - WAITING] Spec generated. Awaiting user review and approval.
[TDD - RED] Creating tests for scenario <Name> -> Running CLI tests.
[TDD - GREEN] Implementing minimal code -> Running CLI tests & type checks.
[TDD - REFACTOR] Refactoring code -> Running Clippy/ESLint & tests.
[OPENSPEC - ARCHIVE] Archiving change into openspec/specs/.
```


## V. CURRENT PROJECT STATE


Single Rust binary (`graphrag`). Hybrid search engine: vector embeddings + knowledge graph over Markdown notes. No Python, no npm, no servers.

### Quick start

```bash
cargo build --release
./target/release/graphrag seed graph.db
./target/release/graphrag search "Python" graph.db -k 5
```

### Commands

| Command | Purpose |
|---------|---------|
| `init db <db>` | Create empty DB with schema |
| `init neovim` | Generate Neovim plugin Lua config |
| `seed <db>` | Populate demo data (44 nodes, ~113 edges) |
| `build <repo> <db>` | Scan `.md` dir, extract entities via Ollama NER, build graph. Default repo: `.` |
| `search <query> <db>` | Hybrid search (vectors + graph expansion) |
| `fts <query> <db>` | FTS5 exact-text search |
| `graph <label> <db>` | Show neighbors of a node |
| `path <from> <to> <db>` | Shortest path between two nodes |
| `stats <db>` | Graph statistics |
| `reset <db>` | Delete DB and recreate empty (rm + init) |
| `mcp --db <db>` | MCP server over stdio (JSON-RPC 2.0) |
| `completions <shell>` | Generate shell completions (bash/zsh/fish) |

### Key flags

| Flag | Applies to | Default | Notes |
|------|-----------|---------|-------|
| `-k` | search | 5 | Number of results |
| `-d` | search, graph | 2 | Graph expansion depth |
| `-a` | search | 0.7 | Vector weight (0.0=pure graph, 1.0=pure vector) |
| `--vector-only` | search | false | Skip graph expansion entirely |
| `--notes-only` | search | false | Show only notes (no entities/tags) |
| `--min-weight` | search | none | Filter edges by minimum weight |
| `--ollama` | search | false | Use Ollama for embeddings (default: synthetic) |
| `--ollama-url` | build, search, mcp | `http://localhost:11434` | |
| `--ner-model` | build | `llama3.2:3b` | Model for entity extraction |
| `--embed-model` | build, search, mcp | `nomic-embed-text` | Model for embeddings |
| `-C` | global | none | Override config file path |
| `num_threads` | config | 4 | Parallel NER workers for build |

### Architecture

```
src/
├── main.rs              ← CLI entrypoint (clap, 11 subcommands)
├── config.rs            ← TOML config (XDG: ~/.config/graphrag/config.toml, auto-created)
├── db/schema.rs         ← SQLite schema + FTS5 (no triggers)
├── graph/
│   ├── build.rs         ← Incremental build (SHA256 hash-based, parallel NER + serial writer)
│   └── expand.rs        ← CTE recursive expansion + shortest path
├── search/hybrid.rs     ← HybridSearch: 3-phase (vector → rerank → graph)
├── embed/ollama.rs      ← Ollama HTTP client (blocking reqwest)
├── vector/
│   ├── mod.rs           ← f32 <-> BLOB, cosine similarity
│   └── synthetic.rs     ← Deterministic hash-based embeddings (no Ollama needed)
├── chunking/markdown.rs ← Frontmatter parsing, section chunking
├── ner/ollama_ner.rs    ← LLM-based entity extraction (batch mode, JSON format, temp=0.1)
├── mcp/mod.rs           ← MCP server (stdio, JSON-RPC 2.0)
└── seed/demo_data.rs    ← Demo data generator
```

### Quirks & gotchas

- **Config file is auto-created** on first run at `$XDG_CONFIG_HOME/graphrag/config.toml` with documented defaults. All fields optional. CLI flags override config.
- **`graphrag.db` default is a sentinel.** If the CLI default `graphrag.db` is used, it falls back to the config file value. This means `graphrag search "foo"` uses the config DB, not a literal `graphrag.db` file.
- **FTS5 is repopulated from scratch** after every `build` (no triggers). This avoids a SQLite 3.x bug where FTS5 `'delete'` fails with empty content.
- **Build is incremental.** Each note stores a SHA256 hash in metadata. Only new/modified files are reprocessed. Deleted files are pruned automatically.
- **Build uses `unchecked_transaction`** per file (not nested). Each file gets its own transaction.
- **Build computes synthetic embeddings** for every node (notes, entities, tags) using deterministic hash-based embeddings. No Ollama needed for search after build.
- **Build is parallel.** N worker threads do NER in parallel, sending results through a channel to a single writer thread (avoids SQLite lock contention). Number of threads configurable via `num_threads` in config.
- **NER batch mode.** All chunks of a file are sent in a single Ollama call with `---SECTION N---` markers. The model returns entities with a `section` field.
- **NER retry with backoff.** On connection error, retries 3 times (2s, 4s, 8s). If all fail, the file is skipped entirely (not inserted without entities).
- **NER requires Ollama.** If Ollama is unreachable or the model is not found, `build` warns per-chunk and continues with empty entities. The `search` command defaults to synthetic embeddings unless `--ollama` is passed.
- **NER model must support plain-text JSON output.** Some models (e.g. some Qwen variants) don't support Ollama's `format: json` mode and return empty responses. The prompt already asks for JSON explicitly — if a model returns empty, try a different model like `llama3.2:3b`.
- **"Thinking" models** (qwen3.5, deepseek-r1) put their response in the `thinking` field instead of `response`. The code falls back to `thinking` and uses regex to extract JSON arrays.
- **Synthetic embeddings** are deterministic (hash-based LCG + Box-Muller). Same text → same vector. No Ollama needed for basic search.
- **MCP server** runs over stdio (JSON-RPC 2.0). Protocol version `2024-11-05`. Exposes 7 tools + 5 resources.

### Module deep-dive

#### `src/main.rs` (720 lines)

Entrypoint CLI con `clap` derive. Define 11 subcomandos + 2 sub-subcomandos de `init`. Implementa la lógica de "sentinel" para `graphrag.db`: si el valor por defecto no se ha cambiado, usa el valor del archivo de configuración. Cada comando delega en una función `cmd_*`.

#### `src/config.rs` (204 lines)

Configuración TOML con auto-creación en `$XDG_CONFIG_HOME/graphrag/config.toml`. Todos los campos opcionales via `#[serde(default)]`. 4 tests unitarios.

#### `src/db/schema.rs` (361 lines)

Esquema SQLite: tabla `nodes` (id, label, type, embedding BLOB, metadata JSON, created_at), tabla `edges` (source_id, target_id, type, weight, context) con FK, índices, y FTS5 virtual table `notes_fts`. Sin triggers — FTS se repuebla desde Rust. 4 tests.

#### `src/graph/build.rs` (829 lines)

Pipeline de construcción del grafo:
1. Escanea `.md` recursivamente con `walkdir`
2. Pre-examina SHA256 vs hashes almacenados en BD (solo procesa nuevos/modificados)
3. Procesamiento paralelo: N workers hacen NER + chunking, envían por canal MPSC a 1 writer thread serial (evita lock contention SQLite)
4. Por archivo: parsea frontmatter, trocea por headers, extrae entidades batch (una llamada Ollama), calcula co-ocurrencias, inserta nodos/aristas en transacción
5. Post-procesado: limpia notas eliminadas, entidades huérfanas, repuebla FTS5
6. `BuildStats` con Display. 4 tests (2 ignorados por depender de Ollama).

#### `src/graph/expand.rs` (261 lines)

Expansión CTE recursiva bidireccional (`expand_neighbors`, `expand_neighbors_weighted`) y camino más corto (`shortest_path`) con protección de ciclos via acumulador de visitados. 3 tests.

#### `src/search/hybrid.rs` (528 lines)

Motor `HybridSearch` con 3 fases:
- **Fase 1 — Vector:** carga todos los embeddings, calcula similitud coseno, top-k
- **Fase 2 — Reranking:** bonifica por coincidencia en título (+0.1) y tipo note (+0.2), combina con alpha
- **Fase 3 — Graph expansion:** CTE recursiva desde candidatos, añade vecinos con score reducido

Normalización min-max final. Soporta `vector_only`, `notes_only`, `min_weight`. También `fts_search` (FTS5) y `stats`.

#### `src/embed/ollama.rs` (123 lines)

Cliente HTTP síncrono (`reqwest::blocking`) para API de embeddings de Ollama. `embed()` llama a `/api/embeddings`, `health_check()` verifica `/api/tags` y que el modelo exista. `embedding_dimension()` heuristico por nombre de modelo.

#### `src/vector/mod.rs` (52 lines)

Operaciones vectoriales: `vector_to_blob`/`blob_to_vector` (bytemuck), `dot`, `norm`, `normalize`, `cosine_similarity`/`cosine_similarity_raw`.

#### `src/vector/synthetic.rs` (132 lines)

Embeddings sintéticos deterministas: hash del texto como seed LCG, Box-Muller para distribución normal, normalización L2. `similar_embedding()` añade ruido gaussiano controlado. 4 tests.

#### `src/chunking/markdown.rs` (187 lines)

Parseo de frontmatter YAML (formato `key: value`), chunking por headers `#`/`##`/`###` con protección de bloques de código (``` fences), descarte de fragmentos <50 chars. `slugify()` con normalización Unicode. 4 tests.

#### `src/ner/ollama_ner.rs` (422 lines)

Extracción de entidades vía LLM (Ollama `/api/generate`). `extract_entities_batch()` envía todos los chunks en una llamada con marcadores `---SECTION N---`. Soporta modelos "thinking" (fallback a campo `thinking`). Parseo robusto: array directo, objeto wrapper, regex. Filtrado por score >= 0.5. 5 tests.

#### `src/mcp/mod.rs` (777 lines)

Servidor MCP sobre stdio (JSON-RPC 2.0, protocolo `2024-11-05`). Expone 7 herramientas: `build`, `search`, `fts`, `graph`, `path`, `stats`, `seed`. Expone 5 recursos: `graphrag://stats`, `graphrag://nodes`, `graphrag://edges`, `graphrag://notes`, `graphrag://entities`. Soporta `graphrag://nodes/<label>` como recurso paramétrico.

#### `src/seed/demo_data.rs` (602 lines)

Generador de base de datos demo: 24 entidades (tools, languages, databases, libraries, frameworks, concepts, security, os), 20 notas con relaciones, ~43 aristas de co-ocurrencia. Embeddings sintéticos deterministas. FTS5 repoblado. 7 tests.

### Testing

```bash
cargo test                    # 32 tests, 2 ignored (need Ollama)
cargo test -- --ignored       # Run Ollama-dependent tests
cargo clippy -- -D warnings   # Lint: zero warnings enforced
cargo fmt --check             # Format check
```

Tests use `tempfile` for temporary DBs. No external services needed for the 30 non-ignored tests.

### Build profile

Release profile in `Cargo.toml`: `lto=true`, `codegen-units=1`, `strip=true`, `opt-level=3`. Binary ~6.5 MB.

### Dependencies

| Crate | Feature | Purpose |
|-------|---------|---------|
| `rusqlite` | `bundled`, `vtab` | SQLite compilado estáticamente, sin dep sistema |
| `reqwest` | `json`, `blocking` | Cliente HTTP para Ollama (síncrono) |
| `tokio` | `full` | Runtime async (para reqwest blocking) |
| `clap` | `derive` | CLI argument parser |
| `clap_complete` | — | Generación de autocompletado para shell |
| `serde` / `serde_json` | `derive` | Serialización JSON |
| `bytemuck` | — | Casting f32 <-> bytes |
| `regex` | — | Expresiones regulares (chunking, NER parsing) |
| `unicode-normalization` | — | Normalización Unicode (slugify) |
| `walkdir` | — | Recorrido recursivo de directorios |
| `anyhow` / `thiserror` | — | Manejo de errores |
| `log` / `env_logger` | — | Logging estructurado |
| `termcolor` / `indicatif` | — | Colores y barras de progreso |
| `rand` | — | RNG para embeddings sintéticos |
| `indexmap` | — | HashMap ordenado |
| `sha2` | — | Hashing SHA256 para build incremental |
| `toml` / `dirs` | — | Config TOML + rutas XDG |
| `tempfile` | dev | Bases de datos temporales en tests |

### Git Flow

This project follows strict gitflow. See [GIT_FLOW.md](./GIT_FLOW.md) for:
- Branch structure (main, development, feature/*, hotfix/*)
- Conventional commits with gitmoji
- How to create features, hotfixes, and releases
- CI/CD workflows for automated versioning and publishing






## Relacionados

- [INFO  graphrag::config](info-graphrag::config.md)
- [DEBUG graphrag](debug-graphrag.md)
- [DEBUG graphrag](debug-graphrag.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::vector::synthetic](debug-graphrag::vector::synthetic.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::graph::expand](debug-graphrag::graph::expand.md)
- [DEBUG graphrag::graph::expand](debug-graphrag::graph::expand.md)
- [DEBUG graphrag::graph::expand](debug-graphrag::graph::expand.md)
- [DEBUG graphrag::graph::expand](debug-graphrag::graph::expand.md)
- [DEBUG graphrag::graph::expand](debug-graphrag::graph::expand.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [DEBUG graphrag::search::hybrid](debug-graphrag::search::hybrid.md)
- [note    ](note-.md)
- [tool    ](tool-.md)
- [device  ](device-.md)
- [format  ](format-.md)
- [class   ](class-.md)
- [domain  ](domain-.md)
- [domain  ](domain-.md)
- [domain  ](domain-.md)
- [note    ](note-.md)
- [tool    ](tool-.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 0](dist-0.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
- [dist 1](dist-1.md)
