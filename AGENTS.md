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

**Publicado en crates.io como `graphrag-search`** (el nombre `graphrag` pertenece a otro proyecto). El binario sigue llamándose `graphrag`:

```bash
cargo install graphrag-search   # instalación desde el registro
```

### Quick start

`seed` **requiere Ollama** (usa `ollama.embed` para entidades, notas y chunks). Sin Ollama falla
con `Error connecting to Ollama`, sale con código no-cero y **no deja ningún fichero de BD** (es
atómico: borra el fichero parcial). Además, `seed` inserta un chunk con embedding por cada nota
demo, así que la **demo ya es buscable**: `search`/`ask`/`similar --label` devuelven resultados
sobre ella (con Ollama en marcha). Para datos reales, usa `build`.

```bash
cargo build --release
./target/release/graphrag seed graph.db        # requiere Ollama
./target/release/graphrag stats graph.db        # no requiere Ollama
./target/release/graphrag fts "Python" graph.db # no requiere Ollama
```

### Commands

| Command | Purpose |
|---------|---------|
| `init db <db>` | Create empty DB with schema |
| `init neovim` | Generate Neovim plugin Lua config |
| `seed <db>` | Populate demo data. **Requiere Ollama**; inserta un chunk con embedding por nota, así que la demo ya es buscable (search/ask/similar devuelven resultados con Ollama) |
| `build <repo> <db>` | Scan `.md` dir, extract entities via Ollama NER, build graph. **Requiere Ollama**. Default repo: `.` |
| `search <query> <db>` | Hybrid search (vectors + graph expansion). **Requiere Ollama** y una BD con chunks (la demo de `seed`, o un grafo via `build`) |
| `ask <query> <db>` | RAG: hybrid retrieval + communities → Ollama answer with cited sources. **Requiere Ollama** y chunks |
| `similar [--label <l>\|--file <f>] <db>` | Similarity search: notes similar to a label or external `.md`. `--label` no requiere Ollama; `--file` sí |
| `fts <query> <db>` | FTS5 exact-text search |
| `graph <label> <db>` | Show neighbors of a node |
| `path <from> <to> <db>` | Shortest path between two nodes |
| `stats <db>` | Graph statistics |
| `reset <db>` | Delete DB and recreate empty (rm + init) |
| `map` | Interactive concept map TUI |
| `community` | Community detection (`detect`) and summarization (`summarize`, **requiere Ollama**) |
| `mcp --db <db>` | MCP server over stdio (JSON-RPC 2.0) |
| `completions <shell>` | Generate shell completions (bash/elvish/fish/powershell/zsh) |

### Key flags

| Flag | Applies to | Default | Notes |
|------|-----------|---------|-------|
| `-k` | search, similar, ask | 5 | Number of results |
| `-d` | search, similar, ask, graph, map | 2 | Graph expansion depth. `-d 0` = solo vectorial (no hay flag `--vector-only`) |
| `-a` | search, ask | 0.7 | Vector weight (0.0=pure graph, 1.0=pure vector) |
| `--notes-only` | search, similar, ask, fts | false | Show only notes (no entities/tags) |
| `--min-weight` | search, similar, ask | none | Filter edges by minimum weight |
| `--format` | search, similar, fts, ask | table | Output format: `table`, `list` (markdown links to notes), `json` |
| `-m, --max-depth` | path | 10 | Maximum path depth |
| `-l, --limit` | fts | 10 | Maximum results |
| `--answer` | search | false | Generate an Ollama answer for the query |
| `--filter` | search, similar, ask | none | Metadata filter expression |
| `--communities` | ask | 3 | Max community summaries in RAG context |
| `--model` | ask | config summary_model | Ollama model for answer generation |
| `--ollama-url` | build, search, ask, seed, mcp, community summarize | `http://localhost:11434` | |
| `--ner-model` | build | `llama3.2:3b` | Model for entity extraction |
| `--embed-model` | build, search, ask, seed, mcp, community summarize | `bge-m3:latest` | Model for embeddings |
| `--db` | mcp | config db | DB path for the MCP server (no posicional) |
| `--resolution` | community detect | 1.0 | Leiden (CPM) resolution parameter |
| `--summary-model` | community summarize | `llama3.2:3b` | Ollama model for community summaries |
| `-C` | global | none | Override config file path |
| `num_threads` | config | 4 | Parallel NER workers for build |

> **Nota:** no existe ningún flag `--ollama` (las embeddings siempre vienen de Ollama; `search`
> siempre embebe la consulta con Ollama) ni ningún flag `--vector-only`.

### Architecture

```
src/
├── main.rs              ← CLI entrypoint (clap, 15 subcommands)
├── config.rs            ← TOML config (XDG: ~/.config/graphrag/config.toml, auto-created)
├── db/
│   ├── schema.rs        ← SQLite schema + FTS5 (no triggers)
│   ├── keys.rs          ← Identidad de nodos: note:<ruta> / node:<label>
│   ├── chunks.rs        ← Almacenamiento/consulta de chunks (tabla `chunks`)
│   └── communities.rs   ← Persistencia de comunidades detectadas
├── graph/
│   ├── build.rs         ← Incremental build (SHA256 hash-based, parallel NER + serial writer)
│   └── expand.rs        ← CTE recursive expansion + shortest path
├── search/
│   ├── hybrid.rs        ← HybridSearch: 3-phase (vector → rerank → graph)
│   └── filter.rs        ← Filtros de metadata (--filter)
├── embed/ollama.rs      ← Ollama HTTP client (blocking reqwest)
├── vector/mod.rs        ← f32 <-> BLOB, cosine similarity (todas las embeddings vía Ollama)
├── chunking/markdown.rs ← Frontmatter parsing, section chunking
├── ner/ollama_ner.rs    ← LLM-based entity extraction (batch mode, JSON format, temp=0.1)
├── map/
│   ├── mod.rs           ← Concept map command
│   ├── layout.rs        ← Force-directed layout (usa `rand`)
│   └── tui.rs           ← Terminal UI interactiva
├── community/
│   ├── detect.rs        ← Detección de comunidades (Leiden, --resolution)
│   ├── summarize.rs     ← Resumen de comunidades vía Ollama (--summary-model)
│   └── search.rs        ← Búsqueda en resúmenes de comunidades
├── seed/demo_data.rs    ← Demo data generator (requiere Ollama; chunks + embeddings por nota)
└── mcp/mod.rs           ← MCP server (stdio, JSON-RPC 2.0)
```

### Quirks & gotchas

- **Config file is auto-created** on first run at `$XDG_CONFIG_HOME/graphrag/config.toml` with documented defaults (db=`graph.db`, ollama_url=`http://localhost:11434`, embed_model=`bge-m3:latest`, ner_model=`llama3.2:3b`, summary_model=`llama3.2:3b`, k=5, depth=2, alpha=0.7, notes_dir=``, num_threads=4). All fields optional. CLI flags override config.
- **`graphrag.db` default is a sentinel.** If the CLI default `graphrag.db` is used, it falls back to the config file value. This means `graphrag search "foo"` uses the config DB, not a literal `graphrag.db` file.
- **FTS5 is repopulated from scratch** after every `build` (no triggers). This avoids a SQLite 3.x bug where FTS5 `'delete'` fails with empty content.
- **Build is incremental.** Each note stores a SHA256 hash in metadata. Only new/modified files are reprocessed. Deleted files are pruned automatically.
- **Build uses `unchecked_transaction`** per file (not nested). Each file gets its own transaction.
- **Build is parallel.** N worker threads do NER in parallel, sending results through a channel to a single writer thread (avoids SQLite lock contention). Number of threads configurable via `num_threads` in config.
- **NER batch mode.** All chunks of a file are sent in a single Ollama call with `---SECTION N---` markers. The model returns entities with a `section` field.
- **NER retry with backoff.** On connection error, retries 3 times (2s, 4s, 8s). If all fail, the file is skipped entirely (not inserted without entities).
- **NER requires Ollama.** If Ollama is unreachable or the model is not found, `build` warns per-chunk and continues with empty entities. `search` **siempre** usa Ollama para embeber la consulta; **no existe** ningún flag `--ollama`.
- **NER model must support plain-text JSON output.** Some models (e.g. some Qwen variants) don't support Ollama's `format: json` mode and return empty responses. The prompt already asks for JSON explicitly — if a model returns empty, try a different model like `llama3.2:3b`.
- **"Thinking" models** (qwen3.5, deepseek-r1) put their response in the `thinking` field instead of `response`. The code falls back to `thinking` and uses regex to extract JSON arrays.
- **No hay modo sin Ollama.** Todas las embeddings provienen de Ollama (`src/vector/synthetic.rs` fue eliminado). No hay embeddings sintéticos ni fallback local.
- **La BD demo (`seed`) ya es buscable.** `seed` inserta, por cada nota demo, una fila en `chunks` con un embedding no nulo (reutiliza el embedding que calcula para el nodo de la nota), así que `search`/`ask`/`similar --label` devuelven resultados sobre ella con Ollama en marcha.
- **`seed` es atómico y requiere Ollama.** Si Ollama no está accesible falla con "Error connecting to Ollama", sale con código no-cero y no deja ningún fichero de BD (se borra el parcial).
- **MCP server** runs over stdio (JSON-RPC 2.0). Protocol version `2024-11-05`. Exposes 7 tools + 5 resources.

### Module deep-dive

#### `src/main.rs` (720 lines)

Entrypoint CLI con `clap` derive. Define **15** subcomandos: `init`, `build`, `similar`, `search`, `ask`, `graph`, `map`, `fts`, `path`, `seed`, `reset`, `mcp`, `stats`, `community`, `completions`. `init` tiene 2 sub-subcomandos (`db`, `neovim`). Implementa la lógica de "sentinel" para `graphrag.db`: si el valor por defecto no se ha cambiado, usa el valor del archivo de configuración. Cada comando delega en una función `cmd_*`.

#### `src/config.rs` (204 lines)

Configuración TOML con auto-creación en `$XDG_CONFIG_HOME/graphrag/config.toml`. Todos los campos opcionales via `#[serde(default)]`. 4 tests unitarios.

#### `src/db/schema.rs` (361 lines)

Esquema SQLite v2: tabla `nodes` (id, `key` TEXT UNIQUE, label, type, embedding BLOB, metadata JSON, created_at), tabla `edges` (source_id, target_id, type, weight, context) con FK, índices (incluido el único `idx_nodes_key`), y FTS5 virtual table `notes_fts`. Sin triggers — FTS se repuebla desde Rust. La identidad es `note:<ruta relativa>` para notas y `node:<label>` para el resto; `label` no es único. Migración v1→v2 idempotente vía `PRAGMA user_version`. 4 tests.

#### `src/db/keys.rs`

Helpers de identidad de nodos: `note_key(ruta)` → `note:<ruta relativa>` y `node_key(label)` → `node:<label>`. Centralizan la construcción de `key` usada en upserts, lookups y migración.

#### `src/db/chunks.rs`

Acceso a la tabla `chunks`: inserción y consulta de fragmentos de notas con su embedding y metadatos. Es la fuente que alimenta la fase vectorial de `search`/`ask`/`similar`.

#### `src/db/communities.rs`

Persistencia de comunidades detectadas (miembros, resumen, embedding del resumen) para `community detect`/`summarize`/`search` y el contexto de `ask`.

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
- **Fase 1 — Vector:** carga los embeddings de **chunks**, calcula similitud coseno, top-k
- **Fase 2 — Reranking:** bonifica por coincidencia en título (+0.1) y tipo note (+0.2), combina con alpha
- **Fase 3 — Graph expansion:** CTE recursiva desde candidatos, añade vecinos con score reducido

Normalización min-max final. Soporta `notes_only` y `min_weight`; `-d 0` desactiva la expansión (solo vectorial). También `fts_search` (FTS5) y `stats`.

#### `src/search/filter.rs`

Evaluación de expresiones de filtro de metadata (`--filter`) aplicadas a los candidatos de búsqueda.

#### `src/embed/ollama.rs` (123 lines)

Cliente HTTP síncrono (`reqwest::blocking`) para API de embeddings de Ollama. `embed()` llama a `/api/embeddings`, `health_check()` verifica `/api/tags` y que el modelo exista. `embedding_dimension()` heuristico por nombre de modelo.

#### `src/vector/mod.rs` (52 lines)

Operaciones vectoriales: `vector_to_blob`/`blob_to_vector` (bytemuck), `dot`, `norm`, `normalize`, `cosine_similarity`/`cosine_similarity_raw`.

#### `src/map/`

`mod.rs` (comando), `layout.rs` (layout force-directed, usa `rand`) y `tui.rs` (interfaz de terminal interactiva) para el mapa de conceptos.

#### `src/community/`

Detección de comunidades (`detect.rs`, Leiden con `--resolution`), resumen vía Ollama (`summarize.rs`, `--summary-model`) y búsqueda sobre los resúmenes (`search.rs`).

#### `src/chunking/markdown.rs` (187 lines)

Parseo de frontmatter YAML (formato `key: value`), chunking por headers `#`/`##`/`###` con protección de bloques de código (``` fences), descarte de fragmentos <50 chars. `slugify()` con normalización Unicode. 4 tests.

#### `src/ner/ollama_ner.rs` (422 lines)

Extracción de entidades vía LLM (Ollama `/api/generate`). `extract_entities_batch()` envía todos los chunks en una llamada con marcadores `---SECTION N---`. Soporta modelos "thinking" (fallback a campo `thinking`). Parseo robusto: array directo, objeto wrapper, regex. Filtrado por score >= 0.5. 5 tests.

#### `src/mcp/mod.rs` (777 lines)

Servidor MCP sobre stdio (JSON-RPC 2.0, protocolo `2024-11-05`). Expone 7 herramientas: `build`, `search`, `fts`, `graph`, `path`, `stats`, `seed`. Expone 5 recursos: `graphrag://stats`, `graphrag://nodes`, `graphrag://edges`, `graphrag://notes`, `graphrag://entities`. Soporta `graphrag://nodes/<label>` como recurso paramétrico.

#### `src/seed/demo_data.rs` (602 lines)

Generador de base de datos demo: 24 entidades (tools, languages, databases, libraries, frameworks, concepts, security, os), 20 notas con relaciones, ~113 aristas de co-ocurrencia (44 nodos). **Requiere Ollama** para calcular las embeddings (de entidades, notas y chunks) vía `ollama.embed`. Inserta embeddings a nivel de nodo y, por cada nota, un chunk con embedding en la tabla `chunks` (mismo embedding que el nodo de la nota), así que la demo es buscable: `search`/`ask`/`similar --label` devuelven resultados. FTS5 repoblado. 7 tests.

### Testing

```bash
cargo test                    # 143 passed, 15 ignored (158 total; los ignorados necesitan Ollama)
cargo test -- --ignored       # Run Ollama-dependent tests
cargo clippy -- -D warnings   # Lint: zero warnings enforced
cargo fmt --check             # Format check
```

Tests use `tempfile` for temporary DBs. Los 143 tests no ignorados no necesitan servicios externos.

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
| `rand` | — | RNG para el layout force-directed del mapa (`src/map/layout.rs`) |
| `indexmap` | — | HashMap ordenado (declarada; verificar uso antes de asumir) |
| `sha2` | — | Hashing SHA256 para build incremental |
| `toml` / `dirs` | — | Config TOML + rutas XDG |
| `tempfile` | dev | Bases de datos temporales en tests |

### Git Flow

This project follows strict gitflow. See [GIT_FLOW.md](./GIT_FLOW.md) for:
- Branch structure (main, development, feature/*, hotfix/*)
- Conventional commits with gitmoji
- How to create features, hotfixes, and releases
- CI/CD workflows for automated versioning and publishing

