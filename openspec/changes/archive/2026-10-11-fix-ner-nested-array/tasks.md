# Tasks: fix-ner-nested-array

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/ner/ollama_ner.rs`: test unitario de `parse_entities_json` con array anidado un nivel (`[[{..}]]`) → devuelve 1 entidad. Nota: pre-fix `parse_entities_json` **devolvía 1 elemento** (el propio array anidado); era `raw_entities_to_entities` quien, al hacer `.get("label")` sobre un array, lo descartaba y producía 0 entidades. El test refuerza esto comprobando el contenido del objeto y el resultado de `raw_entities_to_entities`.
- [x] **RED-2** test unitario de `parse_entities_json` con anidamiento profundo (`[[[{..}]]]`) → devuelve 1 entidad.
- [x] **RED-3** test unitario de `parse_entities_json` con objeto wrapper y array anidado (`{"entities":[[{..}]]}`) → devuelve 1 entidad.
- [x] **RED-4** test unitario de `parse_entities_json` con array plano → devuelve 2 entidades (regresión, debe pasar ya).
- [x] **RED-5** test unitario de `flatten_values` con tipos mixtos y varios niveles → `[obj_A, obj_B, obj_C]` en orden.
- [x] Ejecutar `cargo test` → RED-1/2/3/5 fallan (el helper aún no existe / no se aplica) y los tests existentes siguen VERDE.

### Follow-up (review menores) — RED/GREEN

- [x] **FU-RED-1** estrategia 3 (regex) con array anidado embebido en prosa (`Aquí van las entidades: [[{..}]] fin`) → falla pre-fix (el match lazy captura `[[{..}]` desbalanceado → 0 entidades).
- [x] **FU-RED-2** objeto wrapper con campo `results` (`{"results":[{..}]}`) → 1 entidad (`Go`).
- [x] **FU-RED-3** `flatten_values(vec![])` → `vec![]`.
- [x] **FU-GREEN** estrategia 3 reintenta con un match **greedy** `(?s)\[.*\]` cuando el lazy no parsea y aplica `flatten_values`. No se altera el orden de las estrategias ni el comportamiento de las 1 y 2.

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/ner/ollama_ner.rs`: helper recursivo `flatten_values(Vec<Value>) -> Vec<Value>`.
- [x] **GREEN-2** aplicar `flatten_values` en las 3 estrategias de `parse_entities_json` (array directo, wrapper `entities`/`results`, regex).
- [x] Ejecutar `cargo test` → 100% verde.
- [x] Ejecutar `cargo check` → sin errores.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.

### Verify end-to-end

- [x] `cargo test` → suite completa verde (los `#[ignore]` de Ollama no se ejecutan).
- [x] Verificar que el caso del binario real (`llama3.2:3b`) ya no pierde entidades si devuelve `[[...]]`. (build real OK: Docker/FastAPI/SQLite extraídas; el caso `[[...]]` exacto queda cubierto por unit tests)


### Archive

- [x] Consolidar `entity-extraction` en `openspec/specs/entity-extraction/spec.md` y archivar el change. (archivado 2026-10-11)
