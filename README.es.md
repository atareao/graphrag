# graphrag

**Motor de búsqueda híbrido: embeddings vectoriales + grafo de conocimiento sobre notas Markdown.**

GraphRAG escanea un repositorio de notas Markdown, extrae entidades mediante IA local (Ollama), construye un grafo de conocimiento y permite búsqueda híbrida (vectores semánticos + expansión por grafo) para encontrar artículos relacionados.

Todo en **un único binario Rust** — sin Python, sin npm, sin servidores. 100% local y privado.

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Inicio rápido

```bash
# 1. Instalar
cargo install graphrag

# 2. Poblar datos de demo y probar
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db

# 3. Construir grafo desde tus notas (usa ~/.config/graphrag/config.toml)
graphrag build

# 4. Resetear base de datos (rm + init)
graphrag reset

# 5. Generar autocompletado para shell
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag

# 6. Generar configuración del plugin para Neovim
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua

# 7. Iniciar servidor MCP para Claude Desktop
graphrag mcp --db graph.db
```

---

## Índice

- [Instalación](#instalación)
- [Configuración](#configuración)
- [Comandos CLI](#comandos-cli)
- [Plugin Neovim](#plugin-neovim)
- [Servidor MCP](#servidor-mcp)
- [Arquitectura](#arquitectura)
- [Rendimiento](#rendimiento)
- [Tests](#tests)
- [Depuración](#depuración)

---

## Instalación

### Desde crates.io

```bash
cargo install graphrag
```

### Desde fuente

```bash
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build --release

# Binario en:
#   target/release/graphrag (~6.5 MB)
```

**Dependencias:** Solo Rust ≥ 1.75. SQLite está compilado estáticamente (bundled).

**Opcional:** [Ollama](https://ollama.ai/) con un modelo de embeddings (ej. `bge-m3`) y un LLM para extracción de entidades (ej. `gemma4:e2b`).

---

## Configuración

GraphRAG crea automáticamente un archivo de configuración en `~/.config/graphrag/config.toml` al primer inicio:

```toml
# Ruta por defecto de la base de datos
db = "graph.db"

# URL del servidor Ollama
ollama_url = "http://localhost:11434"

# Modelo de embeddings (usado cuando se pasa --ollama)
embed_model = "bge-m3:latest"

# Modelo para extracción de entidades durante build
ner_model = "gemma4:e2b"

# Número por defecto de resultados de búsqueda
k = 5

# Profundidad de expansión por defecto en el grafo
depth = 2

# Peso vectorial (0.0 = solo grafo, 1.0 = solo vectores)
alpha = 0.7

# Directorio de notas (usado por build y el plugin Neovim)
notes_dir = ""

# Número de hilos paralelos para build (workers NER)
num_threads = 4
```

Todos los campos son opcionales. Las flags de CLI sobrescriben los valores de configuración.

---

## Comandos CLI

```bash
graphrag <COMANDO> [ARGUMENTOS]
```

### Comandos principales

| Comando | Descripción |
|---------|-------------|
| [`build`](#graphrag-build-repo-db) | 📥 Escanear archivos `.md` → extraer entidades → construir grafo |
| [`search`](#graphrag-search-consulta-db) | 🔍 Búsqueda híbrida (vectores + grafo) |
| [`fts`](#graphrag-fts-consulta-db) | 📄 Búsqueda exacta FTS5 |
| [`graph`](#graphrag-graph-etiqueta-db) | 🕸️ Mostrar vecinos de un nodo |
| [`map`](#graphrag-map-db) | 🖥️ Mapa conceptual interactivo TUI |
| [`path`](#graphrag-path-desde-hasta-db) | 🔗 Camino más corto entre dos nodos |
| [`seed`](#graphrag-seed-db) | 🌱 Poblar datos de demo |
| [`stats`](#graphrag-stats-db) | 📊 Estadísticas del grafo |
| [`reset`](#graphrag-reset-db) | 🗑️ Eliminar BD y recrear vacía |

### `graphrag init db [DB]`

Crea una base de datos SQLite vacía con el esquema de GraphRAG.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <ruta>]`

Genera la configuración del plugin Neovim lista para lazy.nvim.

```bash
# Mostrar en stdout
graphrag init neovim

# Guardar a archivo
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

### `graphrag completions <bash|zsh|fish>`

Genera scripts de autocompletado para la shell.

```bash
graphrag completions bash > completions.bash
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
```

### `graphrag seed [DB]`

Puebla la base de datos con datos de demo (44 nodos, ~113 aristas).

```bash
graphrag seed graph.db
graphrag search "Docker security" graph.db -k 5
```

### `graphrag build [REPO] [DB]`

Escanea un directorio de archivos `.md`, extrae entidades mediante Ollama NER y construye el grafo de conocimiento.

**Procesamiento optimizado:**
- **Lote NER:** todos los chunks de un archivo se envían en una sola llamada a Ollama
- **N hilos workers** para NER paralelo + **1 hilo writer** (SQLite serializado, sin contención de bloqueos)
- **Reintento con backoff:** 3 intentos (2s, 4s, 8s) en errores de conexión
- **Incremental:** solo procesa archivos nuevos/modificados (hash SHA256)
- **Limpieza automática:** las notas eliminadas se podan del grafo

```bash
# Construir desde el directorio de notas configurado
graphrag build

# Construir desde un directorio específico
graphrag build ~/notes graph.db

# Construir con un modelo específico
graphrag build ~/notes graph.db --ner-model gemma4:e2b
```

### `graphrag reset [DB]`

Elimina la base de datos y la recrea vacía (rm + init).

```bash
graphrag reset
graphrag reset /data/notes/graph.db
```

### `graphrag search <CONSULTA> [DB]`

Búsqueda híbrida: vectores semánticos + expansión por grafo.

```bash
graphrag search "Python and databases" graph.db -k 10 -d 2
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-k` | 5 | Número de resultados |
| `-d` | 2 | Profundidad de expansión en el grafo |
| `-a` | 0.7 | Peso vectorial (0.0 = solo grafo, 1.0 = solo vectores) |
| `--vector-only` | — | Búsqueda solo vectorial, sin grafo |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |
| `--ollama` | — | Usar Ollama para embeddings (por defecto: synthetic) |

### `graphrag similar --label <ETIQUETA> --file <RUTA> [DB]`

Encuentra notas semánticamente similares. Dos modos:
- `--label`: encuentra notas similares a una nota **ya indexada** (sin Ollama — usa embeddings almacenados)
- `--file`: encuentra notas similares a un archivo `.md` **externo** (chunks y embeddings vía Ollama)

Ambos modos usan **match individual**: cada chunk del origen se compara independientemente, y cada nota destino se puntúa por su mejor par de chunks.

```bash
# Encontrar notas similares a una nota existente
$ graphrag similar --label "Python frameworks" graph.db -k 5

# Encontrar notas similares a un archivo externo
$ graphrag similar --file ~/borradores/nueva-idea.md graph.db -k 10 -d 2
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--label` | — | Etiqueta de una nota existente en la base de datos |
| `--file` | — | Ruta a un archivo `.md` externo |
| `-k` | 5 | Número de resultados |
| `-d` | 2 | Profundidad de expansión en el grafo |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--filter` | — | Filtrar por campo de metadatos (`--filter "category = tutorial"`) |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |

Nota: `--label` y `--file` son mutuamente excluyentes. Usa exactamente uno.

### `graphrag fts <CONSULTA> [DB]`

Búsqueda exacta de texto con FTS5.

```bash
graphrag fts "Python" graph.db -l 20
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-l` | 10 | Número de resultados |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |

### `graphrag graph <ETIQUETA> [DB]`

Muestra los vecinos de un nodo en el grafo.

```bash
graphrag graph "Docker" graph.db -d 2
```

### `graphrag map [DB]`

Mapa conceptual interactivo TUI — explora visualmente el grafo de conocimiento en la terminal.

```bash
# Grafo completo
graphrag map graph.db

# Subgrafo centrado en un nodo
graphrag map --from Python --depth 3 graph.db
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--from` | — | Nodo central (omitir para grafo completo) |
| `-d` | 2 | Profundidad de expansión desde el nodo central |

**Controles:**

| Tecla | Acción |
|-------|--------|
| `↑↓←→` / `hjkl` | Navegar entre nodos |
| `Enter` | Seleccionar nodo (detalles en panel derecho) |
| `t` | Ciclar filtro: todo → notas → entidades → tags |
| `/` | Buscar nodos por nombre |
| `q` / `Esc` | Salir |
| Click mouse | Seleccionar nodo |

### `graphrag path <DESDE> <HASTA> [DB]`

Camino más corto entre dos nodos.

```bash
graphrag path "Python" "Docker" graph.db
```

### `graphrag stats [DB]`

Estadísticas del grafo.

```bash
graphrag stats graph.db
```

### `graphrag mcp`

Inicia un servidor MCP sobre stdio (ver sección MCP).

---

## Plugin Neovim

El plugin `graphrag.nvim` te permite buscar artículos relacionados desde Neovim y abrir resultados o insertarlos como enlaces al final del artículo.

### Instalación

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

lazy.nvim lo carga automáticamente desde `lua/plugins/`.

### Comandos

| Comando | Descripción |
|---------|-------------|
| `:GraphRAG related` | Encuentra notas relacionadas al buffer actual. Ventana flotante. |
| `:GraphRAG insert` | Encuentra relacionadas e inserta enlaces `## Related` al final. |
| `:GraphRAG search <q>` | Búsqueda híbrida |
| `:GraphRAG fts <q>` | Búsqueda textual |
| `:GraphRAG path <A> <B>` | Camino más corto |
| `:GraphRAG stats` | Estadísticas |

### Atajos de teclado

| Tecla | Comando |
|-------|---------|
| `,gr` | `:GraphRAG related` |
| `,gi` | `:GraphRAG insert` |

---

## Servidor MCP

### Configuración para Claude Desktop

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

### Herramientas MCP

| Herramienta | Descripción |
|-------------|-------------|
| `build` | Construir/actualizar el grafo |
| `search` | Búsqueda híbrida |
| `fts` | Búsqueda textual |
| `graph` | Vecinos de un nodo |
| `path` | Camino más corto |
| `stats` | Estadísticas |
| `seed` | Datos de demo |

### Recursos MCP

| URI | Contenido |
|-----|-----------|
| `graphrag://stats` | Estadísticas |
| `graphrag://nodes` | Todos los nodos |
| `graphrag://notes` | Solo notas |
| `graphrag://entities` | Solo entidades |
| `graphrag://edges` | Aristas |

---

## Arquitectura

```
📦 graphrag
├── src/
│   ├── main.rs           ← CLI (clap, 11 subcomandos)
│   ├── config.rs         ← Config TOML (auto-creada)
│   ├── mcp/mod.rs        ← Servidor MCP (JSON-RPC stdio)
│   ├── db/schema.rs      ← Esquema SQLite + FTS5
│   ├── graph/
│   │   ├── build.rs      ← Construcción incremental + paralela
│   │   └── expand.rs     ← CTE recursivo, camino más corto
│   ├── search/hybrid.rs  ← HybridSearch 3 fases
│   ├── embed/ollama.rs   ← Cliente HTTP Ollama
│   ├── vector/
│   │   └── mod.rs        ← Blob <-> Vec<f32>
│   ├── chunking/markdown.rs ← Parseo de frontmatter + chunking
│   ├── ner/ollama_ner.rs ← Extracción de entidades (modo lote)
│   ├── map/
│   │   ├── mod.rs         ← Carga de datos (load_graph, cmd_map)
│   │   ├── layout.rs      ← Algoritmo de layout force-directed
│   │   └── tui.rs         ← TUI interactivo (ratatui)
│   └── seed/demo_data.rs ← Generador de datos de demo
└── target/release/graphrag  ← Binario único (~6.5 MB)
```

### Pipeline de construcción

```
.md → Pre-escaneo (hash SHA256) → Filtrar sin cambios
     → N workers: parsear frontmatter → chunk → lote NER (Ollama)
     → 1 writer: insertar nodos/aristas en SQLite (serializado)
     → Podar notas eliminadas → Repoblar FTS5
```

### Pipeline de búsqueda

```
Consulta → FASE 1: Similitud coseno sobre embeddings
         → FASE 2: Reordenar por título/tipo
         → FASE 3: CTE recursivo de expansión en grafo
         → Resultados con puntuación, contenido, ruta y vecinos
```

---

## Rendimiento

### Modelos recomendados

| Propósito | Modelo | Parámetros | VRAM | Calidad |
|-----------|--------|-------------|------|---------|
| NER | `gemma4:e2b` | 5.1B | ~5GB | Muy buena |
| NER | `gemma4:12b` | 11.9B | ~7GB | Excelente |
| NER | `llama3.2:3b` | 3.2B | ~2GB | Buena (rápida) |
| Embeddings | `bge-m3` | 566M | ~1GB | Excelente |
| Embeddings | `nomic-embed-text` | 137M | ~0.5GB | Buena |

### Rendimiento

Con `gemma4:e2b` y 4 hilos: ~0.08 archivos/s → ~22h para 6254 archivos (solo la primera construcción; las siguientes son incrementales).

### Configuración

```toml
# ~/.config/graphrag/config.toml
ner_model = "gemma4:e2b"
num_threads = 4
```

---

## Tests

```bash
cargo test                    # 123 tests, 12 ignorados (necesitan Ollama)
cargo test -- --ignored       # Tests que dependen de Ollama
```

Los tests usan `tempfile` para bases de datos temporales. No se necesitan servicios externos para los tests no ignorados.

---

## Depuración

```bash
# Logs detallados
RUST_LOG=debug graphrag build

# Filtrar por módulo
RUST_LOG=graphrag::graph::build=debug graphrag build
RUST_LOG=graphrag::ner=debug graphrag build

# Info solamente (por defecto)
graphrag build
```

---

## Licencia

MIT
||||||| 40c468f
<div align="center">

**Hybrid search engine: vector embeddings + knowledge graph over Markdown notes.**

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![Crates.io Downloads](https://img.shields.io/crates/d/graphrag)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org)
[![Built with](https://img.shields.io/badge/built%20with-Rust-1f425f.svg)](https://www.rust-lang.org)

**A single Rust binary — no Python, no npm, no servers. 100% local and private.**

</div>

🇪🇸 *Esta es la traducción al español. La versión original en inglés está en [README.md](./README.md).*

---

## 🧭 GraphRAG para quien no tiene ni idea

*(English version: [README.md](./README.md))*

*(Si ya sabes lo que es RAG, vectores y grafos de conocimiento, sáltate esto y ve directo a [What is Graph RAG?](#-what-is-graph-rag))*

### El problema: tienes cientos de apuntes y no encuentras nada

Imagina que llevas años tomando notas sobre Linux. Tienes una carpeta `~/linux-notes/`
con **500 archivos Markdown** sobre temas como:

```
linux-notes/
├── apt-comandos-basicos.md
├── systemd-crear-servicio.md
├── docker-compose-nginx.md
├── particionar-disco-gpt.md
├── script-bash-backup.md
├── firewall-ufw-reglas.md
├── kernel-compilar-modulo.md
├── ssh-tunel-inverso.md
├── lvm-redimensionar.md
├── nginx-proxy-inverso.md
├── rsync-incremental-backup.md
├── ... (500 archivos)
```

Cuando necesitas encontrar algo, haces lo de siempre:

```bash
grep -ri "particiones" ~/linux-notes/
```

Y funciona... hasta que no funciona. Porque:

- Buscas **"particiones"** pero la nota se titula **"redimensionar LVM"** y nunca usa esa palabra
- Buscas **"monitorizar red"** pero tu nota habla de **"ntopng"** y **"iftop"** sin decir "monitorizar"
- Buscas **"alta disponibilidad"** y te sale una nota suelta, pero te pierdes la de **"keepalived"** y la de **"balanceo nginx"** que están relacionadísimas

**Tu conocimiento está ahí. El problema es que no sabes que está ahí.**

---

### La solución: un buscador que entiende *relaciones*, no solo palabras

GraphRAG escanea tus 500 notas de Linux, extrae automáticamente los conceptos clave
(apt, systemd, LVM, Nginx, Docker, firewalld...) y construye un **mapa de conocimiento**
que sabe cómo se relacionan entre sí.

Ahora, cuando buscas:

```bash
graphrag search "particiones disco duro" linux.db
```

GraphRAG no solo encuentra la nota que menciona "particiones". También te muestra:
- La nota sobre **LVM** (porque LVM y particiones son conceptos vecinos en el grafo)
- La nota sobre **GPT vs MBR** (porque está conectada a través de `particionado`)
- La nota sobre **montar sistema de archivos** (conectada por `disco` → `almacenamiento`)

**Esto es Graph RAG**: un buscador que descubre conexiones que tú mismo no sabías que existían.

---

### Ejemplo real: cómo funciona con tus notas de Linux

#### 1. Construyes el mapa de conocimiento

```bash
# Un solo comando escanea todas tus notas y construye el grafo
graphrag build ~/linux-notes linux.db
```

Detrás de escena, GraphRAG:
1. Lee cada nota y la divide en secciones
2. Envía las secciones a una IA local (Ollama) que extrae entidades como `systemd`,
   `docker`, `nginx`, `particionado`, `firewall`
3. Detecta qué entidades aparecen juntas en las mismas notas (co-ocurrencia)
4. Construye un grafo donde cada entidad es un nodo y cada co-ocurrencia es una conexión
5. Calcula embeddings (vectores) para cada nota y entidad

El resultado es una base de datos SQLite que sabe que:
```
[Docker] ──co-ocurre── [Nginx] ──co-ocurre── [proxy-inverso]
                                              ──co-ocurre── [certbot-ssl]
                                              ──co-ocurre── [balanceo-carga]
```

#### 2. Buscas algo concreto

```bash
graphrag search "servidor web seguro" linux.db -k 5
```

Resultado:
```
 1. Nginx como proxy inverso                Puntuación: 0.92
    Conexiones: nginx, proxy-inverso, http

 2. Certbot: certificados SSL con Let's Encrypt  Puntuación: 0.67
    Conexiones: ssl, certbot, nginx
    ↑ Aparece porque comparte "nginx" con la nota anterior

 3. Fail2ban: proteger SSH y servicios       Puntuación: 0.45
    Conexiones: fail2ban, firewall, seguridad
    ↑ Aparece porque "seguridad" está conectado a "nginx" en el grafo
```

La nota 3 nunca menciona "servidor web" ni "seguro" — pero GraphRAG sabe que
fail2ban se usa para proteger servidores web, porque ambas notas comparten
entidades en el grafo. **grep jamás te habría mostrado esa nota.**

#### 3. Explorar conexiones

```bash
# ¿Qué tiene que ver Docker con systemd?
graphrag path "Docker" "systemd" linux.db
# → Docker → contenedores → systemd → servicios

# ¿Qué entidades rodean a "firewall"?
graphrag graph "firewall" linux.db -d 1
# Vecinos: ufw, iptables, nftables, fail2ban, seguridad, puertos

# Estadísticas generales
graphrag stats linux.db
# → 500 notas, 1200 entidades, 3400 conexiones
```

---

### ¿Y todo esto sin internet?

Sí. GraphRAG es un **binario único de Rust** que:
- No necesita Python, Node.js, ni Docker
- Todo corre en tu máquina, 100% local
- No envía tus datos a ningún servidor
- Si no tienes Ollama, usa embeddings sintéticos (sin IA externa)
- La base de datos SQLite pesa menos que un MP3 y es portátil

**En resumen:** Si tienes cientos de notas en Markdown y estás harto de buscar
con grep o de perderte notas que sabes que existen pero no encuentras,
GraphRAG es el buscador que convierte tu caos de archivos en un mapa
explorable de conocimiento.

---

## 📖 What is Graph RAG?

**Graph RAG** (Retrieval-Augmented Generation with Graphs) is a retrieval technique that improves on plain vector search by incorporating **graph structure** into relevance ranking.

Instead of just finding notes that *mention* your query, it discovers notes connected through **shared concepts and entities** — even if those notes never use the same words.

### Why it matters

| Search method | What it finds | What it misses |
|:---|---|:---|
| **FTS** (full-text) | Exact keyword matches | Synonyms, typos, related concepts |
| **Vector search** | Semantically similar content | Indirect relationships |
| **Graph RAG** | Semantic matches **+** graph-connected content | Nothing — it's strictly better |

**Concrete example:** You have notes about "Docker Compose", "PostgreSQL", and "Patroni HA clusters". Vector search for "PostgreSQL replication" finds the PostgreSQL note. Graph RAG also surfaces the Patroni note — not because it mentions PostgreSQL, but because both connect through the entity `high-availability` in the knowledge graph.

### The three-phase pipeline

```
Query "Python databases"
    │
    ▼
┌──────────────────────────────────────────────────────────┐
│ PHASE 1 — Vector Search                                  │
│ Cosine similarity: query vs every note/entity embedding  │
│ → Top-K candidates ranked by semantic proximity          │
├──────────────────────────────────────────────────────────┤
│ PHASE 2 — Reranking                                      │
│ Boost: title match (+0.1) • note type (+0.2)             │
│ Combine vector score + graph score via α (default 0.7)   │
├──────────────────────────────────────────────────────────┤
│ PHASE 3 — Graph Expansion                                │
│ From top candidates, traverse the knowledge graph         │
│ via recursive CTE (SQLite). Neighbors get decaying score  │
│ → Discovers notes connected through shared entities      │
├──────────────────────────────────────────────────────────┤
│ Results: hybrid score • note content • neighbors         │
└──────────────────────────────────────────────────────────┘
```

> **TL;DR:** Vector search tells you what's similar. Graph expansion tells you what's *connected*. Together they find what you didn't know to look for.

### When to use it

- 🧠 **Personal knowledge bases** — connect ideas across hundreds of notes
- 📘 **Technical documentation** — find related tools, libraries, and concepts
- 🔬 **Research workflows** — discover papers and notes linked by shared entities
- 🌱 **Zettelkasten / digital gardens** — surface implicit links between atomic notes
- 👥 **Team wikis** — find documents connected through shared terminology

---

## ✨ Features

| | Feature | Why it matters |
|---|---|---|
| 🔍 | **Hybrid search** (vectors + graph) | Finds relevant notes *and* connected content you didn't explicitly search for |
| 🧩 | **Entity extraction** via Ollama LLM | Automatically builds the knowledge graph from your notes — no manual tagging |
| 🏘️ | **Community detection** (Leiden algorithm) | Groups related entities into clusters for higher-level understanding |
| 📝 | **Narrative answers** (`--answer`) | Get LLM-generated summaries grounded in community context, not just result lists |
| 📄 | **FTS5 full-text search** | Classic keyword search when you know exactly what you're looking for |
| 🔗 | **Path finding** | Shortest path between any two nodes — discover surprising connections |
| 📊 | **Graph statistics** | Understand your knowledge base structure at a glance |
| ⚡ | **Incremental builds** | Only processes new/modified files (SHA256). Second build is instant |
| 🔄 | **Parallel NER** | N worker threads for entity extraction, 1 serial writer — no SQLite lock contention |
| 🤖 | **MCP server** | Claude Desktop, Cline, and other AI assistants can query your graph in real-time |
| 📝 | **Neovim plugin** | Search and insert related notes without leaving your editor |
| 🚫 | **Zero external deps** | SQLite compiled statically. No Python, Node, Docker, or servers needed |
| 🔒 | **100% local & private** | Everything runs on your machine. No data ever leaves your computer |

---

## 🚀 Quick start (under 2 minutes)

```bash
# 1. Install
cargo install graphrag

# 2. Populate demo data (44 nodes, ~113 edges)
graphrag seed demo.db

# 3. Search — finds notes semantically related to "Python"
graphrag search "Python" demo.db -k 5

# 4. Path — discovers how two technologies connect
graphrag path "Docker" "SQLite" demo.db
# → Docker → virtualization → embedded-database → SQLite

# 5. Graph — see what's connected to a concept
graphrag graph "machine-learning" demo.db -d 2

# 6. Stats — understand your graph
graphrag stats demo.db
```

### Sample output

```
$ graphrag search "Python" demo.db -k 3

Results for "Python" (α=0.70, depth=2):
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 1. Python: A versatile language
    Score: 0.982 | Type: note
    Python is widely used in data science, web development, and automation.
    Neighbors: pandas, flask, machine-learning, data-science

 2. pandas: Data analysis library
    Score: 0.623 | Type: note
    pandas provides high-performance, easy-to-use data structures.
    Neighbors: Python, numpy, data-science, jupyter

 3. Data Science overview
    Score: 0.412 | Type: note
    An introduction to data science concepts and tooling.
    Neighbors: Python, R, machine-learning, statistics
```

### Full demo walkthrough

```bash
# Create a database with demo data
graphrag seed graph.db

# Hybrid search with graph expansion (depth 2)
graphrag search "Docker security" graph.db -k 5

# Search with narrative answer (requires community detect + summarize)
graphrag community detect graph.db
graphrag community summarize graph.db
graphrag search "Docker security" graph.db --answer

# Exact text search
graphrag fts "Python" graph.db -l 10

# Explore the graph
graphrag graph "Docker" graph.db -d 1
graphrag path "Python" "Docker" graph.db
graphrag stats graph.db

# Clean up when done
graphrag reset graph.db
```

---

## 📦 Installation

### From crates.io (recommended)

```bash
cargo install graphrag
```

### From source

```bash
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build --release

# Binary at:
#   target/release/graphrag (~6.5 MB)
```

**Requirements:** Rust ≥ 1.75 only. SQLite is compiled statically via `rusqlite` bundled feature — no system libraries needed.

### Optional: Ollama setup

GraphRAG works **without Ollama** using synthetic (deterministic hash-based) embeddings.
For better results, install [Ollama](https://ollama.ai/) and pull the models you need:

```bash
# Minimal setup (fast, works on any hardware)
ollama pull llama3.2:3b    # NER — 3.2B params, ~2GB VRAM
ollama pull nomic-embed-text  # Embeddings — 137M params, ~0.5GB VRAM

# Recommended setup (best quality)
ollama pull gemma4:e2b      # NER — 5.1B params, ~5GB VRAM
ollama pull bge-m3          # Embeddings — 566M params, ~1GB VRAM
```

---

## ⚙️ Configuration

GraphRAG auto-creates a config file at `~/.config/graphrag/config.toml` on first run.
**All fields are optional** — CLI flags always override config values.

```toml
# =============================================
# GraphRAG configuration
# =============================================

# --- Database ---
db = "graph.db"                    # Default DB path

# --- Ollama connection ---
ollama_url = "http://localhost:11434"
embed_model = "bge-m3:latest"      # Embedding model
ner_model = "gemma4:e2b"           # Entity extraction model

# --- Search defaults ---
k = 5                              # Results count
depth = 2                          # Graph expansion hops
alpha = 0.7                        # Vector weight (0=graph only, 1=vector only)

# --- Notes directory ---
notes_dir = "~/notes"              # Used by `graphrag build` when no dir is given

# --- Build ---
num_threads = 4                    # Parallel NER workers
```

### Practical config examples

<details>
<summary><b>📋 Minimal config (no Ollama, synthetic embeddings only)</b></summary>

```toml
db = "search.db"
notes_dir = "~/notes"
k = 10
depth = 2
# No Ollama fields — uses deterministic synthetic embeddings automatically
```

</details>

<details>
<summary><b>📋 Knowledge base with 1000+ notes</b></summary>

```toml
db = "~/kb/search.db"
notes_dir = "~/kb/notes"
ner_model = "gemma4:e2b"
embed_model = "bge-m3:latest"
k = 10
depth = 2
alpha = 0.7
num_threads = 6
```

</details>

---

## 🛠️ CLI commands

```
graphrag <COMMAND> [ARGS] [OPTIONS]
```

### Core commands

| Command | Description |
|---------|-------------|
| [`build`](#graphrag-build-repo-db) | 📥 Scan `.md` files → extract entities → build knowledge graph |
| [`search`](#graphrag-search-query-db) | 🔍 Hybrid vector + graph search |
| [`fts`](#graphrag-fts-query-db) | 📄 Exact full-text search (FTS5) |
| [`graph`](#graphrag-graph-label-db) | 🕸️ Show neighbors of a node |
| [`path`](#graphrag-path-from-to-db) | 🔗 Shortest path between two nodes |
| [`seed`](#graphrag-seed-db) | 🌱 Populate demo data |
| [`stats`](#graphrag-stats-db) | 📊 Graph statistics |
| [`reset`](#graphrag-reset-db) | 🗑️ Delete DB and recreate empty |

### Advanced commands

| Command | Description |
|---------|-------------|
| [`community detect`](#graphrag-community-detect-db) | 🏘️ Leiden community detection |
| [`community summarize`](#graphrag-community-summarize-db) | 📝 LLM summaries per community |
| [`mcp`](#graphrag-mcp) | 🤖 MCP server for AI assistants |
| [`init db`](#graphrag-init-db-db) | 🆕 Create empty database |
| [`init neovim`](#graphrag-init-neovim---output-path) | 📝 Generate Neovim plugin config |
| [`completions`](#graphrag-completions-bashzshfish) | ⌨️ Shell completions (bash/zsh/fish) |

---

### `graphrag init db [DB]`

Create an empty SQLite database with the GraphRAG schema.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <path>]`

Generate Neovim plugin Lua config ready for [lazy.nvim](https://github.com/folke/lazy.nvim).

```bash
# Print to stdout
graphrag init neovim

# Save directly
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

---

### `graphrag community detect [DB]`

Run **Leiden community detection** on the entity graph — groups related entities into clusters.

```bash
graphrag community detect graph.db

# Coarser communities (lower resolution = fewer, larger groups)
graphrag community detect graph.db --resolution 0.8

# Finer communities (higher resolution = more, smaller groups)
graphrag community detect graph.db --resolution 1.5
```

| Flag | Default | Description |
|------|---------|-------------|
| `--resolution` | 1.0 | CPM quality function resolution parameter |

### `graphrag community summarize [DB]`

Generate LLM summaries for all detected communities. Requires `community detect` first.

```bash
graphrag community summarize graph.db --summary-model gemma4:e2b
```

| Flag | Default | Description |
|------|---------|-------------|
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--summary-model` | `llama3.2:3b` | LLM for generating summaries |
| `--embed-model` | `nomic-embed-text` | Embedding model |

After summarizing, use `graphrag search --answer` to get **narrative answers** grounded in community context.

---

### `graphrag completions <bash|zsh|fish>`

Generate shell completion scripts.

```bash
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
```

---

### `graphrag seed [DB]`

Populate a database with demo data (44 nodes: notes, entities, tags; ~113 edges).

```bash
graphrag seed graph.db
graphrag search "Docker security" graph.db -k 5
```

| Flag | Default | Description |
|------|---------|-------------|
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |

---

### `graphrag build [REPO] [DB]`

The heart of GraphRAG — scan `.md` files, extract entities via Ollama, and build the graph.

**Optimized for large repos:**
| Optimization | How it works |
|:---|---|
| ⚡ **Incremental** | Skips unchanged files via SHA256 hash. Second builds are near-instant |
| 🧵 **Parallel NER** | N worker threads extract entities in parallel |
| ✍️ **Serial writer** | Single thread writes to SQLite — no lock contention |
| 📦 **Batch NER** | All chunks of a file sent in one Ollama call |
| 🔁 **Retry with backoff** | 3 attempts (2s, 4s, 8s) on connection errors |
| 🧹 **Auto-cleanup** | Deleted notes and orphaned entities pruned automatically |

```bash
# Build from configured notes directory
graphrag build

# Build from a specific directory
graphrag build ~/notes graph.db

# Build with specific model
graphrag build ~/notes graph.db --ner-model gemma4:e2b
```

| Flag | Default | Description |
|------|---------|-------------|
| `--embed-model` | `nomic-embed-text` | Embedding model |
| `--ner-model` | `llama3.2:3b` | Entity extraction model |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |

---

### `graphrag reset [DB]`

Delete the database and recreate it empty (equivalent to `rm + init`).

```bash
graphrag reset
graphrag reset /data/notes/graph.db
```

---

### `graphrag search <QUERY> [DB]`

Hybrid search — semantic vectors + graph expansion. This is what makes GraphRAG special.

```bash
graphrag search "Python and databases" graph.db -k 10 -d 2
```

| Flag | Default | Description |
|------|---------|-------------|
| `-k` | 5 | Number of results |
| `-d` | 2 | Graph expansion depth (hops from initial results) |
| `-a` | 0.7 | Vector weight (0.0 = pure graph, 1.0 = pure vector) |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |
| `--notes-only` | — | Show only notes (hide entities and tags) |
| `--min-weight` | — | Minimum edge weight for graph expansion |
| `--filter` | — | Filter by metadata (repeatable). Format: `'field op value'` e.g. `--filter 'date >= 2023'` |
| `--answer` | — | Narrative answer using community context (requires `community detect + summarize`) |
| `--format` | `table` | Output format: `table`, `list`, or `json` |

---

### `graphrag fts <QUERY> [DB]`

Exact full-text search via SQLite FTS5 — classic keyword search.

```bash
graphrag fts "Python" graph.db -l 20
```

| Flag | Default | Description |
|------|---------|-------------|
| `-l` | 10 | Number of results |
| `--notes-only` | — | Show only notes (no entities/tags) |
| `--format` | `table` | Output format: `table`, `list`, or `json` |

---

### `graphrag graph <LABEL> [DB]`

Show neighbors of a node in the knowledge graph. Great for exploring.

```bash
graphrag graph "Docker" graph.db -d 2
```

---

### `graphrag path <FROM> <TO> [DB]`

Shortest path between two nodes — discovers how concepts connect.

```bash
graphrag path "Python" "Docker" graph.db
# → Python → web-development → containerization → Docker
```

| Flag | Default | Description |
|------|---------|-------------|
| `-d` | 10 | Maximum traversal depth |

---

### `graphrag stats [DB]`

Graph statistics. Omits the DB path to use the configured default.

```bash
graphrag stats graph.db
graphrag stats            # uses config default
```

Output example:
```
Graph Statistics
═══════════════════════════════════════
 Nodes:    156
  • notes:      89
  • entities:   52
  • tags:       15
 Edges:    342
 Density:  0.028
 Communities: 12
```

---

### `graphrag mcp`

Start an MCP server over stdio for AI assistant integration (see [MCP section](#-mcp-server)).

```bash
graphrag mcp --db graph.db
graphrag mcp --db graph.db --ollama-url http://localhost:11434
```

| Flag | Default | Description |
|------|---------|-------------|
| `--db` | `graphrag.db` | Path to SQLite database |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |

---

## 🎯 Use cases & examples

### 🔍 Personal knowledge base

You have 300 notes about programming, DevOps, and system design. You search for
"async Rust" and get back notes about tokio and async/await — *plus* a note
about the "actor model" that never mentions those words.

```bash
# Build the graph
graphrag build ~/zettelkasten kb.db

# Hybrid search — finds connected content you didn't explicitly search for
graphrag search "async Rust" kb.db -k 10 -d 2

# Example result you wouldn't get with vector-only search:
# "Actor Model in Distributed Systems" — Score: 0.34 (from graph expansion)
# Connected via: concurrency (shared entity with async Rust notes)
```

### 📚 Technical documentation

Search across multiple project docs and discover cross-project connections.

```bash
graphrag build ~/docs --ner-model gemma4:12b
graphrag search "PostgreSQL high availability" -k 10 -d 3
graphrag path "Kubernetes" "SQLite"
# → Kubernetes → container-orchestration → Docker → embedded-database → SQLite
```

### 🧪 Research paper collection

200 ML paper summaries. Find cross-disciplinary connections.

```bash
graphrag build ~/papers research.db

# Searching for "transformer attention" also surfaces a paper about
# "sparse mixture of experts" — both connect through "self-attention"
graphrag search "transformer attention mechanisms" research.db -k 15 -d 2

# Explore concepts
graphrag graph "self-attention" research.db -d 1
```

### 🔗 Hidden connections

Discover how seemingly unrelated topics connect through your graph.

```bash
# CSS to PostgreSQL? There's a path through web development
graphrag path "CSS" "PostgreSQL"
# → CSS → frontend → web-development → backend → PostgreSQL

# Explore an entity's neighborhood
graphrag graph "machine-learning" -d 2

# Get a narrative answer about a topic
graphrag community detect research.db
graphrag community summarize research.db --summary-model gemma4:e2b
graphrag search "attention mechanisms" research.db --answer
```

### 🤖 AI assistant integration (MCP)

Let Claude, Cline, or any MCP-compatible AI assistant query your knowledge base
in real-time during conversations.

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/usr/local/bin/graphrag",
      "args": ["mcp", "--db", "/home/user/kb.db"]
    }
  }
}
```

Claude can now use tools to explore your knowledge graph:
- `search` — hybrid semantic + graph search 🔍
- `graph` — explore neighbors of a node 🕸️
- `path` — find connections between concepts 🔗
- `stats` — understand graph structure 📊

---

## 📝 Neovim plugin

Search and insert related notes without leaving your editor.

### Installation

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

[lazy.nvim](https://github.com/folke/lazy.nvim) loads it automatically from `lua/plugins/`.

### Commands & keymaps

| Command | Keymap | Description |
|---------|--------|-------------|
| `:GraphRAG related` | `,gr` | Find notes related to current buffer (floating window) |
| `:GraphRAG insert` | `,gi` | Find related and insert `## Related` links at end of file |
| `:GraphRAG search <q>` | — | Hybrid search from command line |
| `:GraphRAG fts <q>` | — | Full-text search |
| `:GraphRAG path <A> <B>` | — | Shortest path |
| `:GraphRAG stats` | — | Graph statistics |

---

## 🤖 MCP server

Expose your knowledge graph to any MCP-compatible AI assistant (Claude Desktop, Cline, etc.).

### Claude Desktop configuration

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/usr/local/bin/graphrag",
      "args": ["mcp", "--db", "/Users/me/knowledge/kb.db"]
    }
  }
}
```

### Tools exposed

| Tool | Description |
|------|-------------|
| `build` | Build or update the graph from your notes directory |
| `search` | Hybrid search (vectors + graph) |
| `fts` | Full-text search |
| `graph` | Show neighbors of a node |
| `path` | Shortest path between two nodes |
| `stats` | Graph statistics |
| `seed` | Populate demo data |

### Resources exposed

| URI | Content |
|-----|---------|
| `graphrag://stats` | Graph statistics |
| `graphrag://nodes` | All nodes |
| `graphrag://notes` | Notes only |
| `graphrag://entities` | Entities only |
| `graphrag://edges` | All edges |
| `graphrag://nodes/<label>` | Single node details |

---

## 🏗️ Architecture

### Module map

```
📦 graphrag  (single ~6.5 MB binary)
├── src/
│   ├── main.rs              ← CLI entrypoint (clap, 12 subcommands)
│   ├── config.rs            ← TOML config (auto-created, all fields optional)
│   ├── mcp/mod.rs           ← MCP server (stdio, JSON-RPC 2.0)
│   ├── db/
│   │   └── schema.rs        ← SQLite schema + FTS5 virtual table
│   ├── graph/
│   │   ├── build.rs         ← Incremental build (parallel NER, serial writer)
│   │   └── expand.rs        ← Recursive CTE expansion + shortest path
│   ├── search/
│   │   └── hybrid.rs        ← 3-phase hybrid search engine
│   ├── embed/
│   │   └── ollama.rs        ← Ollama HTTP client (blocking reqwest)
│   ├── vector/
│   │   ├── mod.rs           ← f32 ↔ BLOB, cosine similarity
│   │   └── synthetic.rs     ← Deterministic hash-based embeddings
│   ├── chunking/
│   │   └── markdown.rs      ← Frontmatter parsing + section chunking
│   ├── ner/
│   │   └── ollama_ner.rs    ← LLM entity extraction (batch mode)
│   └── seed/
│       └── demo_data.rs     ← Demo data generator (44 nodes, 113 edges)
```

### How data flows

```
BUILD PIPELINE:
┌──────────┐   ┌──────────┐   ┌──────────────────────┐   ┌──────────┐
│ .md files│ → │ SHA256   │ → │ N worker threads:     │ → │ 1 writer │
│          │   │ pre-scan │   │ frontmatter → chunk   │   │ thread   │
│          │   │          │   │ → NER batch (Ollama)  │   │ → SQLite │
└──────────┘   └──────────┘   └──────────────────────┘   └──────────┘
                                    ↓                         ↓
                              Entities + co-occurrence    Nodes + Edges
                                                                 ↓
                              ┌─────────────────────────────────┘
                              ↓
                         ┌──────────┐
                         │ Prune    │ → Repopulate FTS5
                         │ deleted  │
                         └──────────┘

SEARCH PIPELINE:
Query → Embed → Cosine similarity → Rerank → Graph expansion → Results
  │        │           │               │            │              │
  │    Ollama or   Top-K scores    +title      Recursive     Hybrid score
  │    synthetic   by semantic     +type       CTE from      + content
  │                 similarity     +alpha      candidates    + neighbors
```

### Database schema

```
┌──────────────────┐       ┌───────────────────────┐
│      nodes       │       │        edges          │
├──────────────────┤       ├───────────────────────┤
│ id        INT PK │──┐    │ source_id INT FK      │
│ label     TEXT   │  └────┤ target_id INT FK      │
│ type      TEXT   │  ┌────┤ type       TEXT        │
│ embedding BLOB  │  │    │ weight     REAL        │
│ metadata  JSON  │  │    │ context    TEXT        │
│ created_at INT  │  │    └───────────────────────┘
└──────────────────┘  │
                      │    ┌───────────────────────┐
      notes_fts       │    │      node_types       │
   (FTS5 virtual)     │    ├───────────────────────┤
                      │    │ note                  │
                      │    │ entity                │
                      │    │ tag                   │
                      └────┤                       │
                           └───────────────────────┘
```

---

## ⚡ Performance

### Recommended Ollama models

| Purpose | Model | Params | VRAM | Quality |
|:---|---:|:---:|:---:|:---:|
| NER | `gemma4:e2b` | 5.1B | ~5GB | ⭐⭐⭐ Very good |
| NER | `gemma4:12b` | 11.9B | ~7GB | ⭐⭐⭐ Excellent |
| NER | `llama3.2:3b` | 3.2B | ~2GB | ⭐⭐ Good (fast) |
| Embeddings | `bge-m3` | 566M | ~1GB | ⭐⭐⭐ Excellent |
| Embeddings | `nomic-embed-text` | 137M | ~0.5GB | ⭐⭐ Good |

### Build throughput

| Scenario | Files | Time | Notes |
|:---|---:|---:|:---|
| First build | 100 | ~20 min | With `gemma4:e2b`, 4 threads |
| First build | 6254 | ~22 h | Large docs repo |
| Incremental | 1 changed | ~2 s | Only re-processes modified files |
| No Ollama | any | ~5 s | Synthetic embeddings, no NER |

> **Tip:** The first build is the slowest (NER is LLM-bound). Subsequent builds are
> near-instant because only new/modified files are processed.

### Memory

| Setup | RAM | Notes |
|:---|---:|:---|
| No Ollama (synthetic) | ~15 MB | Minimal |
| With `nomic-embed-text` | ~500 MB | + Ollama process |
| With `bge-m3` | ~1.5 GB | + Ollama process |
| With `gemma4:e2b` (NER) | ~5 GB | + Ollama process |

---

## 🧪 Tests

```bash
# All tests (30 tests, no external services needed)
cargo test

# Ollama-dependent tests (2 tests, requires Ollama running)
cargo test -- --ignored

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --check
```

Tests use `tempfile` for temporary databases — no cleanup needed.

---

## 🔧 Troubleshooting

### "Ollama is not running"

```
Error: connection refused
```

GraphRAG works without Ollama using synthetic embeddings. If you want Ollama
for better results:

```bash
# Start Ollama
ollama serve

# Verify it's running
curl http://localhost:11434/api/tags
```

### "Model not found"

```
WARN: model "gemma4:e2b" not found, skipping...
```

Pull the model:

```bash
ollama pull gemma4:e2b
```

### Build is slow

The first build runs NER (entity extraction) via LLM on every file. This is
expected — LLMs are slow. After the first build, only changed files are processed.

Tips:
- Increase `num_threads` in config (watch your VRAM)
- Use a smaller NER model: `llama3.2:3b` is 2× faster than `gemma4:e2b`
- Use `--ollama-url` to point to a remote Ollama server on a GPU machine

### "No entities found for any chunks"

Some models don't support Ollama's `format: json` mode. Try:

```bash
# Switch to a model that handles JSON output well
graphrag build --ner-model llama3.2:3b

# Or use gemma4 which has excellent JSON support
graphrag build --ner-model gemma4:e2b
```

### "graphrag.db" uses config but I wanted a literal file

If you pass `graphrag.db` (the default), GraphRAG substitutes the path from
your config file. To force a literal `graphrag.db` file, use the config path:

```toml
# In config.toml
db = "graphrag.db"
```

---

## ❓ FAQ

<details>
<summary><b>Does GraphRAG need Ollama?</b></summary>
No. Without Ollama, GraphRAG uses deterministic synthetic embeddings (hash-based).
Search still works — just without the semantic quality that LLM embeddings provide.
</details>

<details>
<summary><b>Can I use it with non-Markdown files?</b></summary>
Currently only `.md` files are supported. The build pipeline parses Markdown
frontmatter and section headers for chunking.
</details>

<details>
<summary><b>How is this different from Microsoft's GraphRAG?</b></summary>
Microsoft GraphRAG is a Python project that uses global community summarization
for question-answering over large datasets. This project is a lightweight Rust
binary focused on personal knowledge bases — instant search, no server, single
binary, works entirely offline.
</details>

<details>
<summary><b>Can I share the database between machines?</b></summary>
Yes — the SQLite database is portable. Copy it between machines and it works.
Synthetic embeddings are deterministic (same text = same vector), so builds
are reproducible across machines.
</details>

<details>
<summary><b>How do I update the graph when I add new notes?</b></summary>
Just run `graphrag build` again. It only processes new and modified files
(checked via SHA256 hash). Deleted files are auto-pruned.
</details>

<details>
<summary><b>Does it work with multiple users?</b></summary>
The SQLite database supports concurrent readers. For write access (build),
only one process at a time.
</details>

---

## 🤝 Contributing

Contributions are welcome! Here's how to get started:

```bash
# Clone and build
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build

# Run tests
cargo test

# Check lint
cargo clippy -- -D warnings

# Format
cargo fmt --check
```

### Development tips

- Use `RUST_LOG=debug graphrag build` to see detailed build logs
- The demo data seed is at `src/seed/demo_data.rs` — great starting point
- All CLI commands are in `src/main.rs` — easy to add new ones
- SQLite schema is in `src/db/schema.rs`

---

## 📚 Further reading

- [Microsoft GraphRAG paper](https://arxiv.org/abs/2404.16130) — the original research
- [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html) — full-text search engine
- [Leiden algorithm](https://arxiv.org/abs/1810.08473) — community detection used by GraphRAG
- [Ollama](https://ollama.ai/) — local LLM server

---

## 📄 License

MIT — see [LICENSE](LICENSE).

---

<div align="center">
  <sub>Built with 🦀 Rust • SQLite • Ollama</sub>
</div>
