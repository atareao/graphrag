# Guía de Usuario — GraphRAG

**GraphRAG** es un motor de búsqueda híbrida que combina **vectores semánticos** (embeddings) con un **grafo de conocimiento** para encontrar notas relacionadas en tus apuntes Markdown.

Todo vive en **un único binario Rust** llamado `graphrag`: sin Python, sin npm, sin servidores. 100% local y privado.

Está publicado en [crates.io](https://crates.io/crates/graphrag-search) como **`graphrag-search`** (el nombre `graphrag` ya pertenece a otro proyecto). El binario resultante, no obstante, se llama `graphrag`.

Versión actual: **0.2.12**.

---

## Índice

1. [Instalación](#1-instalación)
2. [Primeros pasos](#2-primeros-pasos)
3. [Configuración](#3-configuración)
4. [Cómo funciona la búsqueda híbrida](#4-cómo-funciona-la-búsqueda-híbrida)
5. [Comandos CLI](#5-comandos-cli)
6. [Filtros de metadatos](#6-filtros-de-metadatos)
7. [Formatos de salida](#7-formatos-de-salida)
8. [Comunidades y respuestas RAG](#8-comunidades-y-respuestas-rag)
9. [Autocompletado para el shell](#9-autocompletado-para-el-shell)
10. [Plugin de NeoVim](#10-plugin-de-neovim)
11. [Servidor MCP](#11-servidor-mcp)
12. [Ejemplos prácticos](#12-ejemplos-prácticos)
13. [Preguntas frecuentes](#13-preguntas-frecuentes)
14. [Referencia rápida](#14-referencia-rápida)
15. [Resolución de problemas](#15-resolución-de-problemas)

---

## 1. Instalación

### Desde crates.io (recomendado)

```bash
cargo install graphrag-search
```

Esto instala el binario **`graphrag`** en tu `~/.cargo/bin/`.

### Desde el código fuente

```bash
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build --release
```

El binario queda en `target/release/graphrag` (~6.5 MB). Cópialo a tu `PATH` si quieres:

```bash
cp target/release/graphrag ~/.local/bin/
# o, para todo el sistema:
sudo cp target/release/graphrag /usr/local/bin/
```

### Requisitos

- **Rust ≥ 1.75** ([rustup.rs](https://rustup.rs/)) — solo si compilas desde fuente.
- **SQLite** se compila estáticamente dentro del binario (feature `bundled` de `rusqlite`), así que **no necesitas ninguna librería del sistema**.

### Ollama (necesario para embeddings)

**Ya no existe un modo sin Ollama.** Todas las embeddings provienen de Ollama; no hay embeddings generados localmente ni de respaldo. Sin un servidor Ollama en marcha solo funcionan los comandos que trabajan sobre datos ya almacenados o que no usan vectores:

- **No requieren Ollama:** `init db`, `init neovim`, `reset`, `completions`, `stats`, `graph`, `path`, `map`, `fts`, `similar --label` y `community detect`.
- **Sí requieren Ollama:** `seed`, `build`, `search`, `ask`, `similar --file` y `community summarize`.

Instala [Ollama](https://ollama.ai/) y descarga los modelos que necesites:

```bash
# Configuración mínima (rápida, funciona en cualquier equipo)
ollama pull llama3.2:3b        # NER — 3.2B parámetros, ~2 GB
ollama pull nomic-embed-text   # Embeddings — 137M parámetros, ~0.5 GB

# Configuración recomendada (mejor calidad)
ollama pull gemma4:e2b         # NER — 5.1B parámetros, ~5 GB
ollama pull bge-m3             # Embeddings — 566M parámetros, ~1 GB
```

| Propósito | Modelo | Parámetros | VRAM aprox. |
|-----------|--------|-----------:|-------------|
| Extracción de entidades (NER) | `llama3.2:3b` | 3.2B | ~2 GB |
| Extracción de entidades (NER) | `gemma4:e2b` | 5.1B | ~5 GB |
| Embeddings | `nomic-embed-text` | 137M | ~0.5 GB |
| Embeddings | `bge-m3` | 566M | ~1 GB |

> No hay ningún flag `--ollama`: la conexión se controla con `--ollama-url` y `--embed-model` / `--ner-model`. Ver la sección [Preguntas frecuentes](#13-preguntas-frecuentes) para saber qué comandos la necesitan.

---

## 2. Primeros pasos

### 2.1. Probar con los datos de demostración

La base de datos de demostración **requiere Ollama**: `seed` lo usa para generar las embeddings de nodos y de chunks. Si Ollama no está en marcha, `seed` falla con error de conexión y **no crea la BD** (es atómico: no deja ningún fichero parcial).

Una vez sembrada, es **directamente buscable**: sirve tanto para los comandos sobre el grafo ya construido (`fts`, `graph`, `path`, `stats`, `map`) como para la búsqueda híbrida (`search`, `ask`, `similar --label`), siempre con Ollama en marcha. Para una demo funcional necesitas tener Ollama en marcha y un modelo de embeddings descargado.

```bash
# Crea una BD con 44 nodos (24 entidades + 20 notas) y ~113 aristas
graphrag seed graph.db

# Búsqueda híbrida sobre la demo (requiere Ollama)
graphrag search "Python" graph.db -k 5

# Búsqueda textual exacta (FTS5)
graphrag fts "Python" graph.db

# Vecinos de un nodo en el grafo
graphrag graph "Docker" graph.db -d 2

# Camino más corto entre dos conceptos
graphrag path "Docker" "SQLite" graph.db

# Estadísticas del grafo
graphrag stats graph.db
```

### 2.2. Construir el grafo desde tus notas

```bash
graphrag build ~/notas graph.db
```

Esto escanea recursivamente `~/notas` en busca de archivos `.md`, divide cada nota en secciones, extrae entidades con Ollama (NER) y construye el grafo de conocimiento. El build es **incremental**: en ejecuciones posteriores solo procesa archivos nuevos o modificados.

Si en tu configuración has definido `notes_dir`, basta con:

```bash
graphrag build
```

### 2.3. Buscar en tu propio grafo

```bash
graphrag search "seguridad en contenedores" graph.db -k 10 -d 2
```

> `search` necesita Ollama en ejecución **y** que el `build` previo se haya hecho **con Ollama**. Si el build se hizo sin Ollama, no se generan embeddings de chunk y la búsqueda vectorial/híbrida no encontrará el contenido de esas notas hasta reconstruir con Ollama.

### 2.4. Preguntar en lenguaje natural (RAG)

```bash
graphrag ask "¿cómo configuro nginx como proxy inverso?" graph.db
```

---

## 3. Configuración

### 3.1. Archivo de configuración

GraphRAG crea automáticamente un archivo de configuración en el primer arranque:

- **XDG:** `$XDG_CONFIG_HOME/graphrag/config.toml`
- **Linux por defecto:** `~/.config/graphrag/config.toml`

Contenido por defecto **exacto**:

```toml
# GraphRAG Configuration
# Auto-generated. All fields are optional — CLI flags override these values.

# Default database path
db = "graph.db"

# Ollama server URL
ollama_url = "http://localhost:11434"

# Embedding model (used when --ollama is passed)
embed_model = "bge-m3:latest"

# Model for entity extraction during build
ner_model = "llama3.2:3b"

# Model for community summarization
summary_model = "llama3.2:3b"

# Default number of search results
k = 5

# Default graph expansion depth
depth = 2

# Vector weight (0.0 = pure graph, 1.0 = pure vector)
alpha = 0.7

# Notes directory (used by the Neovim plugin)
notes_dir = ""

# Number of parallel threads for build (NER workers)
num_threads = 4
```

**Todos los campos son opcionales.** Los flags de CLI siempre tienen prioridad sobre el archivo.

| Campo | Para qué sirve |
|-------|----------------|
| `db` | Ruta de la base de datos por defecto (evita el "sentinel", ver 3.3). |
| `ollama_url` | URL del servidor Ollama. |
| `embed_model` | Modelo de embeddings. |
| `ner_model` | Modelo de extracción de entidades durante `build`. |
| `summary_model` | Modelo para resumir comunidades (y para `ask` si no pasas `--model`). |
| `k` | Número de resultados por defecto. |
| `depth` | Profundidad de expansión por defecto en el grafo. |
| `alpha` | Peso vectorial (0.0 = solo grafo, 1.0 = solo vector). |
| `notes_dir` | Directorio de notas (lo usa el plugin de NeoVim y `build` cuando no indicas repo). |
| `num_threads` | Workers NER en paralelo durante `build`. |

### 3.2. Usar un archivo de configuración alternativo

```bash
graphrag -C /ruta/mi-config.toml search "consulta" db
```

`-C` / `--config` es una opción **global**: se coloca antes del subcomando.

### 3.3. El "sentinel" de la base de datos

Aunque el `--help` muestra `graphrag.db` como valor por defecto del argumento `DB`, GraphRAG aplica una regla especial:

> Si **no indicas** la base de datos, o escribes literalmente `graphrag.db`, se usa el valor del campo `db` del archivo de configuración.

Es decir, `graphrag search "foo"` **no** busca necesariamente en un archivo llamado `graphrag.db` del directorio actual, sino en tu BD configurada. Para forzar una ruta concreta, pásala explícitamente (por ejemplo `graphrag search "foo" mi.db`) o cambia `db` en el config.

---

## 4. Cómo funciona la búsqueda híbrida

`search`, `ask` y `similar` comparten un motor de 3 fases:

```
Consulta
   │
   ▼
┌──────────────────────────────────────────────────────────┐
│ FASE 1 — Búsqueda vectorial                              │
│ Similitud coseno entre la consulta y los embeddings      │
│ almacenados. Se obtienen las K notas candidatas.         │
├──────────────────────────────────────────────────────────┤
│ FASE 2 — Reranking                                       │
│ Bonus por coincidencia en el título (+0.1) y por ser     │
│ nota (+0.2). Se combina la puntuación vectorial con la   │
│ del grafo mediante α (por defecto 0.7).                  │
├──────────────────────────────────────────────────────────┤
│ FASE 3 — Expansión por grafo                             │
│ Desde las notas candidatas se recorre el grafo con una   │
│ CTE recursiva (SQLite). Los vecinos entran con una       │
│ puntuación decreciente según la profundidad y el peso.   │
└──────────────────────────────────────────────────────────┘
   │
   ▼
Normalización min-max final → resultados con puntuación,
contenido, ruta y vecinos.
```

### Parámetros clave

| Flag | Efecto |
|------|--------|
| `-k` / `--k` | Número de resultados finales. |
| `-d` / `--depth` | Profundidad de expansión por grafo. **`-d 0` desactiva la expansión**: búsqueda solo vectorial. |
| `-a` / `--alpha` | Peso vectorial. `0.0` = solo grafo, `1.0` = solo vector. |
| `--min-weight` | Peso mínimo de arista para que un vecino entre en la expansión. |
| `--notes-only` | Oculta entidades y tags; solo notas. |

> **Importante:** `search` **necesita Ollama** para embeber la consulta. No hay fallback sin Ollama en tiempo de búsqueda: si Ollama no responde, `search` falla con error de conexión. Para búsqueda puramente vectorial sobre embeddings ya almacenados usa `-d 0`.

En `search`, la fase de comunidades relacionadas (cuando están resumidas) muestra como máximo **2** comunidades relacionadas. Para obtener una respuesta narrativa en `search`, usa `--answer` (ver [sección 8](#8-comunidades-y-respuestas-rag)).

---

## 5. Comandos CLI

Todos los comandos aceptan la opción global `-C, --config <CONFIG>`. Además existen `-h/--help`, `-V/--version`.

```
graphrag <COMANDO> [ARGUMENTOS] [OPCIONES]
```

### 5.1. `graphrag init db [DB]`

Crea una base de datos SQLite vacía con el esquema de GraphRAG.

- `DB` por defecto: `graphrag.db`

```bash
graphrag init db graph.db
```

### 5.2. `graphrag init neovim [-o|--output <OUTPUT>]`

Genera la configuración del plugin de NeoVim (formato lazy.nvim).

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-o, --output <OUTPUT>` | stdout | Ruta de salida. Si se indica, escribe el archivo **y crea el directorio del plugin**. |

```bash
# Imprimir por stdout
graphrag init neovim

# Guardar el archivo
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

### 5.3. `graphrag build [REPO] [DB]`

Escanea un directorio de notas `.md`, extrae entidades mediante Ollama (NER) y construye el grafo.

- `REPO` por defecto: `.` (directorio actual). Si usas `REPO="."` y tienes `notes_dir` configurado, se usa `notes_dir`.
- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |
| `--ner-model <NER_MODEL>` | `llama3.2:3b` | Modelo para extracción de entidades. |
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |

**Características del build:**

- **Incremental:** compara el hash SHA256 de cada archivo y solo reprocesa los nuevos o modificados.
- **Paralelo:** N hilos workers hacen NER y envían resultados por canal a **1 hilo escritor** serial (sin contención de bloqueos en SQLite). N se controla con `num_threads` del config.
- **NER en lote:** todos los chunks de un archivo se envían en una única llamada a Ollama con marcadores `---SECTION N---`.
- **Reintentos con backoff:** 3 intentos (2 s, 4 s, 8 s) ante errores de conexión. Si todos fallan, el archivo se omite por completo.
- **Poda automática:** las notas eliminadas y las entidades huérfanas se eliminan del grafo.
- **FTS5:** se repuebla desde cero después de cada build.

Si Ollama no está disponible, `build` avisa por chunk y continúa sin extraer entidades. Las notas se indexan igualmente, pero quedan **sin entidades y sin embeddings de chunk** (el propio build avisa: *"File processed without chunk embeddings — vector search for this file's content will not work until embeddings..."*). No hay embeddings de respaldo. Por tanto la búsqueda vectorial/híbrida sobre el contenido de esas notas **no funcionará** hasta reconstruir con `build` y Ollama en marcha.

```bash
# Build desde el directorio configurado en notes_dir
graphrag build

# Build desde un directorio concreto
graphrag build ~/notas graph.db

# Build con modelos específicos
graphrag build ~/notas graph.db \
  --ollama-url http://localhost:11434 \
  --ner-model llama3.2:3b \
  --embed-model nomic-embed-text
```

### 5.4. `graphrag similar [DB]`

Encuentra notas semánticamente similares a una nota ya indexada o a un archivo externo.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--label <LABEL>` | — | Etiqueta de una nota ya indexada. **No necesita Ollama**: usa los embeddings almacenados. |
| `--file <FILE>` | — | Ruta a un archivo `.md` externo. **Sí necesita Ollama** para trocear y embeber. |
| `-k, --k <K>` | `5` | Número de resultados. |
| `-d, --depth <DEPTH>` | `2` | Profundidad de expansión en el grafo. |
| `--min-weight <MIN_WEIGHT>` | — | Peso mínimo de arista para expansión. |
| `--notes-only` | — | Solo notas (sin entidades ni tags). |
| `--filter <EXPR>` | — | Filtro de metadatos (repetible). |
| `--format <FORMAT>` | `table` | `table` \| `list` \| `json`. |

**Regla:** debes indicar **exactamente uno** de `--label` o `--file`. Si indicas ambos o ninguno, el comando falla con error y sale con código 1.

```bash
# Similar a una nota existente (sin Ollama)
graphrag similar --label "Python frameworks" graph.db -k 5

# Similar a un archivo externo (necesita Ollama)
graphrag similar --file ~/borradores/nueva-idea.md graph.db -k 10 -d 2
```

### 5.5. `graphrag search <QUERY> [DB]`

Búsqueda híbrida (vectores + grafo).

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-k, --k <K>` | `5` | Número de resultados. |
| `-d, --depth <DEPTH>` | `2` | Profundidad de expansión. `0` = solo vectorial. |
| `-a, --alpha <ALPHA>` | `0.7` | Peso vectorial (0.0–1.0). |
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |
| `--min-weight <MIN_WEIGHT>` | — | Peso mínimo de arista para expansión. |
| `--notes-only` | — | Solo notas (sin entidades ni tags). |
| `--filter <EXPR>` | — | Filtro de metadatos (repetible). |
| `--answer` | — | Respuesta narrativa usando contexto de comunidades (requiere `community detect` + `community summarize`). |
| `--format <FORMAT>` | `table` | `table` \| `list` \| `json`. |

> No existe `--vector-only` ni `--ollama`. Para búsqueda solo vectorial, usa `-d 0`. La conexión a Ollama se controla con `--ollama-url`.

```bash
graphrag search "Python y bases de datos" graph.db -k 10 -d 2
graphrag search "Python" graph.db -d 0          # solo vectorial
graphrag search "Linux" graph.db -a 0.0          # solo grafo
graphrag search "backups" graph.db --filter 'date >= 2024'
```

### 5.6. `graphrag ask <QUERY> [DB]`

RAG en un solo comando: retrieval híbrido + resúmenes de comunidades + evidencia de chunks → respuesta en lenguaje natural generada con Ollama y **fuentes citadas**.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-k, --k <K>` | `5` | Resultados de retrieval usados como evidencia. |
| `-d, --depth <DEPTH>` | `2` | Profundidad de expansión. `0` = solo vectorial. |
| `-a, --alpha <ALPHA>` | `0.7` | Peso vectorial (0.0–1.0). |
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |
| `--min-weight <MIN_WEIGHT>` | — | Peso mínimo de arista para expansión. |
| `--notes-only` | — | Solo notas (sin entidades ni tags). |
| `--filter <EXPR>` | — | Filtro de metadatos (repetible). |
| `--communities <N>` | `3` | Máximo de resúmenes de comunidad en el contexto. |
| `--model <MODEL>` | `summary_model` del config | Modelo Ollama para la generación. |
| `--format <FORMAT>` | `table` | `table` \| `list` \| `json`. |

**Comportamiento:**

- Si **Ollama no responde**, `ask` degrada a retrieval puro y sale con **código 0** (no falla).
- Si **no hay comunidades resumidas**, avisa y continúa (la respuesta se genera solo con las notas recuperadas).
- Con `--format json`, la salida es un objeto: `{ "answer", "sources", "results" }`.
- Para que el contexto de comunidades funcione, ejecuta antes `graphrag community detect` y `graphrag community summarize`.

```bash
graphrag ask "¿cómo configuro nginx?" graph.db
graphrag ask "backups" graph.db -k 10 --filter 'date >= 2024'
graphrag ask "nginx" graph.db --communities 5 --model llama3.2:3b
graphrag ask "nginx" graph.db --format json
```

### 5.7. `graphrag graph <LABEL> [DB]`

Muestra los vecinos de un nodo en el grafo.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-d, --depth <DEPTH>` | `2` | Profundidad de expansión. |

```bash
graphrag graph "Docker" graph.db -d 2
```

### 5.8. `graphrag map [DB]`

Mapa conceptual interactivo (TUI con `ratatui`).

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--from <FROM>` | — | Nodo central opcional. Sin él, se muestra el grafo completo. |
| `-d, --depth <DEPTH>` | `2` | Profundidad de expansión desde el nodo central. |

**Controles:**

| Tecla | Acción |
|-------|--------|
| `↑↓←→` o `hjkl` | Navegar entre nodos. |
| `Enter` | Seleccionar / previsualizar el nodo. |
| `t` | Ciclar filtro: todo → notas → entidades → tags. |
| `/` | Buscar por nombre. |
| `q` / `Esc` | Salir. |
| Click de ratón | Seleccionar un nodo. |

El peso de arista se muestra en el modo de visualización "id".

```bash
# Grafo completo
graphrag map graph.db

# Subgrafo centrado en un nodo
graphrag map --from Python --depth 3 graph.db
```

### 5.9. `graphrag fts <QUERY> [DB]`

Búsqueda textual exacta con FTS5.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-l, --limit <LIMIT>` | `10` | Número de resultados. |
| `--notes-only` | — | Solo notas (oculta entidades y tags). |
| `--format <FORMAT>` | `table` | `table` \| `list` \| `json`. |

```bash
graphrag fts "Python" graph.db -l 20
```

### 5.10. `graphrag path <FROM> <TO> [DB]`

Camino más corto entre dos nodos.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-m, --max-depth <MAX_DEPTH>` | `10` | Profundidad máxima de recorrido. |

> **Ojo:** la opción es **`-m`**, no `-d`.

```bash
graphrag path "Python" "Docker" graph.db
graphrag path "Python" "Docker" graph.db -m 5
```

### 5.11. `graphrag seed [DB]`

Genera una base de datos de demostración con datos de ejemplo: **44 nodos** (24 entidades + 20 notas) y **~113 aristas**. **Requiere Ollama**: genera las embeddings de nodos y de chunks con él. Si Ollama no está en marcha, el comando falla y no crea la base de datos (es **atómico**: no deja ningún fichero parcial).

> **Nota:** por cada nota demo inserta una fila en la tabla `chunks` con su embedding, de modo que la BD demo queda lista para `search`, `ask` y `similar --label` (con Ollama en marcha), además de `fts`, `graph`, `path` y `stats`.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |

```bash
graphrag seed graph.db
```

### 5.12. `graphrag reset [DB]`

Borra la base de datos (incluyendo los archivos auxiliares `-wal` y `-shm`) y la recrea vacía. Al terminar, sugiere ejecutar `build`.

- `DB` por defecto: `graphrag.db`

```bash
graphrag reset
graphrag reset /data/notas/graph.db
```

### 5.13. `graphrag stats [DB]`

Muestra estadísticas del grafo. `DB` es opcional; si no se indica, se usa el valor de la config.

```bash
graphrag stats graph.db
graphrag stats            # usa la BD configurada
```

### 5.14. `graphrag mcp`

Inicia un servidor MCP sobre stdio (JSON-RPC 2.0). **No recibe la base de datos como argumento posicional**; se pasa con `--db`.

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--db <DB>` | `graphrag.db` | Ruta a la base de datos SQLite. |
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |

```bash
graphrag mcp --db graph.db
```

Ver la [sección 11](#11-servidor-mcp) para la configuración completa.

### 5.15. `graphrag community detect [DB]`

Ejecuta detección de comunidades (algoritmo **Leiden**) sobre el grafo de entidades. Se puede elegir el nivel de granularidad.

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--resolution <RESOLUTION>` | `1.0` | Parámetro de resolución de la función de calidad CPM de Leiden. |

```bash
graphrag community detect graph.db
graphrag community detect graph.db --resolution 1.5   # más comunidades, más pequeñas
graphrag community detect graph.db --resolution 0.8   # menos comunidades, más grandes
```

### 5.16. `graphrag community summarize [DB]`

Genera resúmenes LLM por comunidad. **Requiere haber ejecutado antes `community detect`.**

- `DB` por defecto: `graphrag.db`

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--ollama-url <OLLAMA_URL>` | `http://localhost:11434` | URL de Ollama. |
| `--summary-model <SUMMARY_MODEL>` | `llama3.2:3b` | Modelo para generar los resúmenes. |
| `--embed-model <EMBED_MODEL>` | `nomic-embed-text` | Modelo de embeddings. |

```bash
graphrag community summarize graph.db
graphrag community summarize graph.db --summary-model gemma4:e2b
```

### 5.17. `graphrag completions <SHELL>`

Genera scripts de autocompletado para el shell. Valores posibles: **`bash`, `elvish`, `fish`, `powershell`, `zsh`**.

```bash
graphrag completions bash > completions.bash
```

Ver la [sección 9](#9-autocompletado-para-el-shell).

---

## 6. Filtros de metadatos

Los comandos `search`, `ask` y `similar` aceptan `--filter <EXPR>` (repetible).

**Sintaxis:** `'campo operador valor'`.

- **Operadores:** `=`, `!=`, `>=`, `<=`, `>`, `<`.
- El **campo** admite *dotted paths* (`a.b`), útil para metadatos anidados.
- Si el **valor** es numérico se compara como número; si no, como texto.
- Los filtros se aplican sobre el JSON `metadata` de los nodos.
- Varios `--filter` se combinan con **AND**.
- Una sintaxis inválida aborta el comando con error.

```bash
# Rango de fechas
graphrag search "backups" graph.db --filter 'date >= 2023'

# Igualdad de texto
graphrag search "docker" graph.db --filter 'category = tutorial'

# Varios filtros combinados (AND)
graphrag search "nginx" graph.db \
  --filter 'status != archived' \
  --filter 'priority < 5'
```

---

## 7. Formatos de salida

Los comandos `search`, `ask`, `similar` y `fts` aceptan `--format`:

| Valor | Formato |
|-------|---------|
| `table` | Tabla legible (por defecto). |
| `list` | Lista de enlaces Markdown a las notas: `- [Título](ruta)`. |
| `json` | Array de resultados. En `ask`, un objeto `{ "answer", "sources", "results" }`. |

```bash
graphrag search "Python" graph.db --format list
graphrag fts "Docker" graph.db --format json
graphrag ask "nginx" graph.db --format json
```

El formato `list` es especialmente cómodo para pegar resultados como enlaces Markdown en tus notas.

---

## 8. Comunidades y respuestas RAG

GraphRAG puede agrupar las entidades de tu grafo en **comunidades** (clústeres) y resumir cada una con el LLM. Esto habilita respuestas de nivel superior, no solo listas de notas.

### 8.1. Detectar y resumir comunidades

```bash
# 1. Detectar comunidades (Leiden)
graphrag community detect graph.db

# 2. Resumir cada comunidad con Ollama
graphrag community summarize graph.db
```

Puedes ajustar la granularidad con `--resolution` en `detect` y el modelo con `--summary-model` en `summarize`.

### 8.2. Respuesta narrativa en `search`

Con las comunidades ya resumidas:

```bash
graphrag search "Docker security" graph.db --answer
```

`--answer` usa el contexto de comunidades para generar una respuesta narrativa. La fase de comunidades relacionadas muestra como máximo 2 comunidades.

### 8.3. RAG completo con `ask`

`ask` hace todo el flujo en un solo comando: retrieval híbrido + resúmenes de comunidades + evidencia de chunks → respuesta con fuentes.

```bash
graphrag ask "¿cómo aseguro un contenedor Docker?" graph.db
graphrag ask "nginx" graph.db --communities 5 --model llama3.2:3b
```

Si Ollama cae, `ask` degrada a retrieval puro y sale con código 0. Si no hay comunidades resumidas, avisa y continúa.

---

## 9. Autocompletado para el shell

GraphRAG genera scripts de autocompletado para **bash**, **zsh**, **fish**, **elvish** y **powershell**.

### Bash

```bash
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag
# o a nivel de sistema:
graphrag completions bash | sudo tee /usr/share/bash-completion/completions/graphrag
```

### Zsh

```bash
graphrag completions zsh > /usr/local/share/zsh/site-functions/_graphrag
# o con oh-my-zsh:
graphrag completions zsh > ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/graphrag/_graphrag
```

### Fish

```bash
graphrag completions fish > ~/.config/fish/completions/graphrag.fish
```

### Otros

```bash
graphrag completions elvish > graphrag.elv
graphrag completions powershell > graphrag.ps1
```

---

## 10. Plugin de NeoVim

El plugin `graphrag.nvim` te permite buscar notas relacionadas desde NeoVim y abrir los resultados o insertarlos como enlaces al final del artículo.

### 10.1. Generar la configuración

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

Si indicas `--output`, GraphRAG escribe el archivo **y crea el directorio del plugin**. Si lo omites, el Lua se imprime por stdout.

El Lua generado usa los valores de tu config (`db`, `k`, `depth`, `notes_dir`) y está pensado para cargarse con **lazy.nvim** desde `lua/plugins/`.

### 10.2. Comandos disponibles

| Comando | Descripción |
|---------|-------------|
| `:GraphRAG related` | Busca notas relacionadas con el buffer actual. Ventana flotante. |
| `:GraphRAG insert` | Busca relacionadas e inserta una sección `## Relacionados` con enlaces al final. |
| `:GraphRAG sel` | Búsqueda híbrida usando la **selección visual** como consulta. |
| `:GraphRAG ftsel` | Búsqueda FTS usando la **selección visual** como consulta. |
| `:GraphRAG search <q>` | Búsqueda híbrida desde la línea de comandos. |
| `:GraphRAG fts <q>` | Búsqueda textual exacta. |
| `:GraphRAG path <A> <B>` | Camino más corto entre dos nodos. |
| `:GraphRAG stats` | Estadísticas del grafo. |

### 10.3. Atajos de teclado

| Atajo | Comando |
|-------|---------|
| `,gr` | `:GraphRAG related` |
| `,gi` | `:GraphRAG insert` |

### 10.4. Flujo de uso típico

1. Abres `docker/seguridad-en-docker.md`.
2. Pulsas `,gi`.
3. El plugin ejecuta una búsqueda de notas relacionadas y añade al final:

```markdown
## Relacionados

- [AppArmor en Ubuntu](apparmor-en-ubuntu.md)
- [Hardening de imágenes Docker](hardening-de-imagenes-docker.md)
```

Con `related` (o `,gr`) los resultados se muestran en una ventana flotante en lugar de insertarse.

---

## 11. Servidor MCP

GraphRAG incluye un servidor MCP (Model Context Protocol) sobre stdio (JSON-RPC 2.0), para que asistentes de IA como **Claude Desktop** puedan consultar tu grafo.

### 11.1. Configuración en Claude Desktop

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/ruta/completa/graphrag",
      "args": ["mcp", "--db", "/ruta/completa/graph.db"]
    }
  }
}
```

> Recuerda: `mcp` **no** recibe la BD como argumento posicional; se pasa con `--db`.

### 11.2. Herramientas (tools) expuestas

| Herramienta | Descripción |
|-------------|-------------|
| `build` | Construye o actualiza el grafo. |
| `search` | Búsqueda híbrida (vectores + grafo). |
| `fts` | Búsqueda textual FTS5. |
| `graph` | Vecinos de un nodo. |
| `path` | Camino más corto entre dos nodos. |
| `stats` | Estadísticas del grafo. |
| `seed` | Datos de demostración. |

### 11.3. Recursos (resources) expuestos

| URI | Contenido |
|-----|-----------|
| `graphrag://stats` | Estadísticas del grafo. |
| `graphrag://nodes` | Todos los nodos. |
| `graphrag://edges` | Todas las aristas. |
| `graphrag://notes` | Solo notas. |
| `graphrag://entities` | Solo entidades. |
| `graphrag://nodes/<label>` | Detalle de un nodo concreto (recurso paramétrico). |

---

## 12. Ejemplos prácticos

### 12.1. Explorar la demo de cero en un minuto

> La BD demo es buscable: puedes usar `search` (con Ollama en marcha), además de `fts`, `graph`, `path` y `stats`. El ejemplo de búsqueda híbrida sobre una base construida con `build` está en la sección 12.2.

```bash
graphrag seed demo.db
graphrag search "Python" demo.db -k 5
graphrag fts "Python" demo.db
graphrag graph "Docker" demo.db -d 2
graphrag path "Docker" "SQLite" demo.db
graphrag stats demo.db
```

### 12.2. Construir y buscar en tu base de conocimiento

```bash
# Construir el grafo desde tus notas (con Ollama)
graphrag build ~/notas kb.db --ner-model llama3.2:3b

# Búsqueda híbrida
graphrag search "async Rust" kb.db -k 10 -d 2

# Solo vectorial
graphrag search "bases de datos" kb.db -d 0

# Solo grafo
graphrag search "contenedores" kb.db -a 0.0
```

### 12.3. Descubrir conexiones ocultas

```bash
# ¿Cómo se conectan dos conceptos?
graphrag path "CSS" "PostgreSQL" kb.db

# Vecindario de una entidad
graphrag graph "machine-learning" kb.db -d 2

# Estadísticas generales
graphrag stats kb.db
```

### 12.4. Búsqueda textual y filtros

```bash
graphrag fts "Docker" kb.db -l 20 --notes-only
graphrag search "backups" kb.db --filter 'date >= 2024' --format list
graphrag search "tutoriales" kb.db \
  --filter 'category = tutorial' \
  --filter 'status != archived'
```

### 12.5. Respuestas con RAG

```bash
graphrag community detect kb.db
graphrag community summarize kb.db --summary-model llama3.2:3b
graphrag ask "¿cómo aseguro un contenedor Docker?" kb.db
graphrag search "Docker security" kb.db --answer
```

### 12.6. Similaridad

```bash
# Nota similar a una ya indexada (sin Ollama)
graphrag similar --label "Nginx proxy inverso" kb.db -k 5

# Nota similar a un borrador externo (con Ollama)
graphrag similar --file ~/borradores/keepalived.md kb.db -k 10
```

### 12.7. Exportar al portapapeles como Markdown

```bash
graphrag search "Python" kb.db --format list
```

### 12.8. Limpiar y reconstruir

```bash
graphrag reset kb.db
graphrag build ~/notas kb.db
```

---

## 13. Preguntas frecuentes

### ¿Necesito Ollama?

Depende del comando:

- **No lo necesitan** (trabajan sobre datos ya almacenados o no usan vectores):
  - `init db`, `init neovim`, `reset`, `completions`, `stats`, `graph`, `path`, `map`, `fts`, `similar --label`, y `community detect`.
- **Sí lo necesitan** (hay que embeber texto o generar con LLM):
  - `seed` (embeddings de nodos y chunks), `build` (NER + embeddings de chunk), `search` (embebe la consulta), `ask` (embebe y genera), `similar --file` (trocea y embebe), y `community summarize` (resumen LLM).

No existe un flag `--ollama`: la conexión se controla con `--ollama-url` y los modelos `--embed-model` / `--ner-model` / `--summary-model`.

> **Matiz importante:** `search`/`ask` necesitan Ollama **en ejecución** para embeber la consulta. Para tus propias notas, además, los embeddings de chunk se generan durante `build`, así que el grafo debe haberse construido **con Ollama**; si el build se hizo sin Ollama, `search` no encontrará nada aunque Ollama esté levantado, y habrá que reconstruir con Ollama. La BD de demostración (`seed`) sí incluye chunks con embedding, por lo que es buscable directamente (con Ollama en marcha).

### Si Ollama está caído, ¿qué pasa?

- `seed`: **falla** con error de conexión y no crea la base de datos.
- `search` y `similar --file`: **fallan** con error de conexión (no hay fallback sin Ollama).
- `community summarize`: **falla** (necesita el LLM para resumir).
- `ask`: **degrada** a retrieval puro y sale con código **0**.
- `build`: avisa por chunk y continúa, pero las notas quedan sin entidades ni embeddings de chunk.

### ¿Qué modelos de Ollama se recomiendan?

| Propósito | Modelo | Tamaño |
|-----------|--------|--------|
| NER | `llama3.2:3b` (rápido) | ~2 GB |
| NER | `gemma4:e2b` (mejor calidad) | ~5 GB |
| Embeddings | `nomic-embed-text` | ~0.5 GB |
| Embeddings | `bge-m3` | ~1 GB |

### ¿Los modelos "thinking" funcionan?

Sí. Modelos como `qwen3.5` o `deepseek-r1` responden en el campo `thinking` en lugar de `response`; el parser tiene un *fallback* por regex que extrae el JSON. Aun así, los modelos de NER deben soportar salida JSON en texto plano. Si un modelo devuelve respuestas vacías con `format: json`, prueba con `llama3.2:3b`.

### ¿Dónde se guarda la base de datos?

Si no indicas una ruta, se usa el campo `db` del archivo de configuración (por defecto `graph.db`). Recuerda el "sentinel": escribir `graphrag.db` literal equivale a usar el valor del config.

### ¿El build es incremental?

Sí. Solo reprocesa archivos nuevos o modificados (hash SHA256). Las notas eliminadas y las entidades huérfanas se podan automáticamente.

### ¿Puedo usarlo sin conexión?

Sí, para los comandos que no usan vectores ni LLM (ver [¿Necesito Ollama?](#¿necesito-ollama)). **No hay modo offline**: todo el pipeline de embeddings pasa por Ollama. Eso sí, Ollama corre en tu propia máquina, así que todo sigue siendo 100% local y nada sale de tu equipo.

### ¿Qué tamaño tiene el binario?

~6.5 MB, un único binario estático sin dependencias de sistema.

### ¿Cómo obtengo solo resultados vectoriales?

Usa `-d 0`. No existe `--vector-only`.

---

## 14. Referencia rápida

```bash
# --- Instalación ---
cargo install graphrag-search
cargo build --release                       # desde fuente

# --- Demo (buscable con Ollama) ---
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag fts "Python" graph.db
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db

# --- Construir desde tus notas ---
graphrag build ~/notas graph.db --ner-model llama3.2:3b

# --- Búsqueda ---
graphrag search "tema" graph.db -k 10 -d 2
graphrag search "tema" graph.db -d 0                      # solo vectorial
graphrag search "tema" graph.db -a 0.0                    # solo grafo
graphrag search "tema" graph.db --filter 'date >= 2024'
graphrag fts "Docker" graph.db -l 20

# --- Grafo ---
graphrag graph "Docker" graph.db -d 2
graphrag path "Python" "Docker" graph.db -m 5
graphrag map --from Python --depth 3 graph.db

# --- Similaridad ---
graphrag similar --label "Nota existente" graph.db -k 5
graphrag similar --file ~/borrador.md graph.db -k 10

# --- RAG ---
graphrag community detect graph.db
graphrag community summarize graph.db
graphrag ask "¿cómo configuro nginx?" graph.db
graphrag search "nginx" graph.db --answer

# --- Utilidades ---
graphrag init db graph.db
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag
graphrag reset graph.db
graphrag mcp --db graph.db
```

### Tabla resumen de todos los comandos

| Comando | Argumentos | Flags propios |
|---------|-----------|---------------|
| `init db` | `[DB]` | — |
| `init neovim` | — | `-o/--output` |
| `build` | `[REPO] [DB]` | `--embed-model`, `--ner-model`, `--ollama-url` |
| `similar` | `[DB]` | `--label`, `--file`, `-k`, `-d`, `--min-weight`, `--notes-only`, `--filter`, `--format` |
| `search` | `<QUERY> [DB]` | `-k`, `-d`, `-a`, `--ollama-url`, `--embed-model`, `--min-weight`, `--notes-only`, `--filter`, `--answer`, `--format` |
| `ask` | `<QUERY> [DB]` | `-k`, `-d`, `-a`, `--ollama-url`, `--embed-model`, `--min-weight`, `--notes-only`, `--filter`, `--communities`, `--model`, `--format` |
| `graph` | `<LABEL> [DB]` | `-d` |
| `map` | `[DB]` | `--from`, `-d` |
| `fts` | `<QUERY> [DB]` | `-l`, `--notes-only`, `--format` |
| `path` | `<FROM> <TO> [DB]` | `-m/--max-depth` |
| `seed` | `[DB]` | `--ollama-url`, `--embed-model` |
| `reset` | `[DB]` | — |
| `stats` | `[DB]` | — |
| `mcp` | — | `--db`, `--ollama-url`, `--embed-model` |
| `community detect` | `[DB]` | `--resolution` |
| `community summarize` | `[DB]` | `--ollama-url`, `--summary-model`, `--embed-model` |
| `completions` | `<SHELL>` | — |

---

## 15. Resolución de problemas

### Ollama no responde

```
Error: connection refused
```

Comprueba que está en marcha y que la URL es la correcta:

```bash
ollama serve
curl http://localhost:11434/api/tags
```

Recuerda que `seed`, `search`, `similar --file` y `community summarize` fallan si Ollama no está; `ask` degrada a retrieval; `build` continúa sin entidades ni embeddings de chunk.

### "Model not found"

```
WARN: model "llama3.2:3b" not found, skipping...
```

Descarga el modelo:

```bash
ollama pull llama3.2:3b
```

### El build es lento

El NER está limitado por el LLM. Ajusta `num_threads` en la configuración y usa un modelo más pequeño y rápido (`llama3.2:3b`). Los builds posteriores son casi instantáneos gracias al modo incremental.

### Salida vacía del NER

Algunos modelos no soportan el modo `format: json` de Ollama y devuelven respuestas vacías. Prueba con `llama3.2:3b` u otro modelo que soporte JSON en texto plano.

### Búsqueda con resultados extraños

Ajusta los pesos: sube `-a` (más vectorial) o bájalo (más grafo), y controla la expansión con `-d`. Usa `--min-weight` para descartar aristas débiles.

### Depuración con logs

```bash
RUST_LOG=debug graphrag build
RUST_LOG=graphrag::graph::build=debug graphrag build
RUST_LOG=graphrag::ner=debug graphrag build
```

---

**Licencia:** MIT.
