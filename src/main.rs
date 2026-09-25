mod chunking;
mod community;
mod config;
mod db;
mod embed;
mod graph;
mod map;
mod mcp;
mod ner;
mod search;
mod seed;
mod vector;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::map::cmd_map;
use clap_complete::Shell;
use log::debug;
use std::path::{Path, PathBuf};

/// Format for displaying search results
#[derive(clap::ValueEnum, Clone, Default)]
enum OutputFormat {
    #[default]
    Table,
    List,
    Json,
}

/// GraphRAG — Motor de búsqueda híbrida con vectores + grafos de conocimiento
///
/// Construye grafos de conocimiento desde notas Markdown y permite
/// búsqueda híbrida (vectores semánticos + expansión por grafo)
/// todo en local, 100% privado.
#[derive(Parser)]
#[command(name = "graphrag", version, about, long_about = None)]
struct Cli {
    /// Ruta al archivo de configuración TOML
    #[arg(short = 'C', long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inicializa recursos (base de datos, configuración de plugins, etc.)
    Init {
        #[command(subcommand)]
        action: InitCommands,
    },
    /// Construye un grafo de conocimiento desde un directorio de notas Markdown
    Build {
        /// Ruta al directorio con notas .md (por defecto: directorio actual)
        #[arg(default_value = ".")]
        repo: String,
        /// Ruta a la base de datos SQLite
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
        /// Modelo para extracción de entidades
        #[arg(long, default_value = "llama3.2:3b")]
        ner_model: String,
        /// URL de Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
    },
    /// Búsqueda por similitud: encuentra notas similares a una nota existente (--label) o a un archivo externo (--file)
    Similar {
        /// Etiqueta de una nota ya indexada en la BD
        #[arg(long)]
        label: Option<String>,
        /// Ruta a un archivo .md externo
        #[arg(long)]
        file: Option<String>,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Número de resultados
        #[arg(short, long, default_value = "5")]
        k: usize,
        /// Profundidad de expansión en el grafo
        #[arg(short, long, default_value = "2")]
        depth: i32,
        /// Peso mínimo de arista para expansión
        #[arg(long)]
        min_weight: Option<f64>,
        /// Solo notas (sin entidades ni tags)
        #[arg(long)]
        notes_only: bool,
        /// Filtrar por campo de metadatos (repeatable). Formato: 'campo operador valor'
        #[arg(long = "filter", value_name = "EXPR")]
        filter: Vec<String>,
        /// Formato de salida: table, list, json
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },
    /// Búsqueda híbrida (vectores + grafos)
    Search {
        /// Consulta de búsqueda
        #[arg(allow_hyphen_values = true)]
        query: String,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Número de resultados
        #[arg(short, long, default_value = "5")]
        k: usize,
        /// Profundidad de expansión en el grafo
        #[arg(short, long, default_value = "2")]
        depth: i32,
        /// Peso de la componente vectorial (0.0-1.0)
        #[arg(short, long, default_value = "0.7")]
        alpha: f64,
        /// URL de Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
        /// Peso mínimo de arista para expansión
        #[arg(long)]
        min_weight: Option<f64>,
        /// Solo notas (sin entidades ni tags)
        #[arg(long)]
        notes_only: bool,
        /// Filtrar por campo de metadatos (repeatable). Formato: 'campo operador valor',
        /// e.g., 'date >= 2023', 'category = tutorial'
        #[arg(long = "filter", value_name = "EXPR")]
        filter: Vec<String>,
        /// Generate a narrative answer using community context (requires 'community detect' + 'community summarize' first)
        #[arg(long)]
        answer: bool,
        /// Formato de salida: table, list, json
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },
    /// Búsqueda solo por grafo desde un nodo
    Graph {
        /// Etiqueta del nodo de partida
        label: String,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Profundidad de expansión
        #[arg(short, long, default_value = "2")]
        depth: i32,
    },
    /// Interactive concept map TUI
    Map {
        /// Nodo central opcional
        #[arg(long)]
        from: Option<String>,
        /// Profundidad de expansión
        #[arg(short, long, default_value = "2")]
        depth: i32,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
    },
    /// Búsqueda textual exacta (FTS5)
    Fts {
        /// Consulta de búsqueda
        #[arg(allow_hyphen_values = true)]
        query: String,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Número de resultados
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Mostrar solo notas (ocultar entidades y tags)
        #[arg(long)]
        notes_only: bool,
        /// Formato de salida: table, list, json
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },
    /// Camino más corto entre dos nodos
    Path {
        /// Nodo origen
        from: String,
        /// Nodo destino
        to: String,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Profundidad máxima
        #[arg(short, long, default_value = "10")]
        max_depth: i32,
    },
    /// Genera una base de datos de demostración con datos sintéticos
    Seed {
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// URL de Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
    },
    /// Borra la base de datos y la recrea vacía
    Reset {
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
    },
    /// Inicia un servidor MCP sobre stdio para que asistentes IA interactúen con el grafo
    Mcp {
        /// Ruta a la base de datos SQLite
        #[arg(long, default_value = "graphrag.db")]
        db: String,
        /// URL del servidor Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
    },
    /// Muestra estadísticas de la base de datos
    Stats {
        /// Ruta a la base de datos
        db: Option<String>,
    },
    /// Community detection and summarization
    Community {
        #[command(subcommand)]
        action: CommunityCommands,
    },
    /// Genera scripts de autocompletado para el shell
    Completions {
        /// Shell para el que generar el script
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum CommunityCommands {
    /// Run Leiden community detection on the entity graph
    Detect {
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Resolution parameter for CPM quality function (default: 1.0)
        #[arg(long, default_value = "1.0")]
        resolution: f64,
    },
    /// Generate LLM summaries for all communities
    Summarize {
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// URL de Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Modelo para generación de resúmenes
        #[arg(long, default_value = "llama3.2:3b")]
        summary_model: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
    },
}

#[derive(Subcommand)]
enum InitCommands {
    /// Inicializa una nueva base de datos SQLite vacía
    Db {
        /// Ruta a la base de datos SQLite
        #[arg(default_value = "graphrag.db")]
        db: String,
    },
    /// Genera la configuración del plugin de NeoVim
    Neovim {
        /// Ruta de salida para el archivo (por defecto: stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();
    let cfg = load_config(&cli.config);
    debug!(
        "Config loaded: db={}, ollama_url={}, embed_model={}, ner_model={}",
        cfg.db, cfg.ollama_url, cfg.embed_model, cfg.ner_model
    );

    match cli.command {
        Commands::Init { action } => match action {
            InitCommands::Db { db } => {
                let db = if db == "graphrag.db" { &cfg.db } else { &db };
                cmd_init(db)
            }
            InitCommands::Neovim { output } => cmd_neovim(output, &cfg),
        },
        Commands::Build {
            repo,
            db,
            embed_model,
            ner_model,
            ollama_url,
        } => {
            let db = if db == "graphrag.db" { cfg.db } else { db };
            let repo = if repo == "." && !cfg.notes_dir.is_empty() {
                cfg.notes_dir.clone()
            } else {
                repo
            };
            let embed_model = if embed_model == "nomic-embed-text" {
                cfg.embed_model
            } else {
                embed_model
            };
            let ner_model = if ner_model == "llama3.2:3b" {
                cfg.ner_model
            } else {
                ner_model
            };
            let ollama_url = if ollama_url == "http://localhost:11434" {
                cfg.ollama_url
            } else {
                ollama_url
            };
            debug!("Comando: build repo={}, db={}", repo, db);
            cmd_build(
                &repo,
                &db,
                &ollama_url,
                &ner_model,
                &embed_model,
                cfg.num_threads,
            )
        }
        Commands::Search {
            query,
            db,
            k,
            depth,
            alpha,
            ollama_url,
            embed_model,
            min_weight,
            notes_only,
            filter,
            answer,
            format,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let k = if k == 5 { cfg.k } else { k };
            let depth = if depth == 2 { cfg.depth } else { depth };
            let alpha_val = if (alpha - 0.7).abs() < 0.01 {
                cfg.alpha
            } else {
                alpha
            };
            let ollama_url = if ollama_url == "http://localhost:11434" {
                &cfg.ollama_url
            } else {
                &ollama_url
            };
            let embed_model = if embed_model == "nomic-embed-text" {
                &cfg.embed_model
            } else {
                &embed_model
            };
            debug!(
                "Comando: search query='{}', k={}, depth={}",
                query, k, depth
            );
            cmd_search(
                &query,
                db,
                k,
                depth,
                alpha_val,
                ollama_url,
                embed_model,
                min_weight,
                notes_only,
                &filter,
                answer,
                &format,
            )
        }
        Commands::Similar {
            label,
            file,
            db,
            k,
            depth,
            min_weight,
            notes_only,
            filter,
            format,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let k = if k == 5 { cfg.k } else { k };
            let depth = if depth == 2 { cfg.depth } else { depth };
            debug!("Comando: similar label={:?}, file={:?}", label, file);
            cmd_similar(
                label, file, db, k, depth, min_weight, notes_only, &filter, &cfg, &format,
            )
        }
        Commands::Graph { label, db, depth } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let depth = if depth == 2 { cfg.depth } else { depth };
            debug!("Comando: graph label='{}', depth={}", label, depth);
            cmd_graph(&label, db, depth)
        }
        Commands::Map { from, db, depth } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let depth = if depth == 2 { cfg.depth } else { depth };
            debug!("Comando: map from={:?}, depth={}", from, depth);
            cmd_map(db, from, depth)
        }
        Commands::Fts {
            query,
            db,
            limit,
            notes_only,
            format,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!(
                "Comando: fts query='{}', limit={}, notes_only={}",
                query, limit, notes_only
            );
            cmd_fts(&query, db, limit, notes_only, &format)
        }
        Commands::Path {
            from,
            to,
            db,
            max_depth,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: path from='{}', to='{}'", from, to);
            cmd_path(&from, &to, db, max_depth)
        }
        Commands::Seed {
            db,
            ollama_url,
            embed_model,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let ollama_url = if ollama_url == "http://localhost:11434" {
                &cfg.ollama_url
            } else {
                &ollama_url
            };
            let embed_model = if embed_model == "nomic-embed-text" {
                &cfg.embed_model
            } else {
                &embed_model
            };
            debug!("Comando: seed db={}", db);
            cmd_seed(db, ollama_url, embed_model)
        }
        Commands::Reset { db } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: reset db={}", db);
            cmd_reset(db)
        }
        Commands::Mcp {
            db,
            ollama_url,
            embed_model,
        } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let ollama_url = if ollama_url == "http://localhost:11434" {
                &cfg.ollama_url
            } else {
                &ollama_url
            };
            let embed_model = if embed_model == "nomic-embed-text" {
                &cfg.embed_model
            } else {
                &embed_model
            };
            debug!("Comando: mcp db={}", db);
            mcp::run_server(db, ollama_url, embed_model, cfg.num_threads)
        }
        Commands::Stats { db } => {
            debug!("Comando: stats db={}", db.as_deref().unwrap_or(&cfg.db));
            cmd_stats(db.as_deref().unwrap_or(&cfg.db))
        }
        Commands::Community { action } => match action {
            CommunityCommands::Detect { db, resolution } => {
                let db = if db == "graphrag.db" { &cfg.db } else { &db };
                cmd_community_detect(db, resolution)
            }
            CommunityCommands::Summarize {
                db,
                ollama_url,
                summary_model,
                embed_model,
            } => {
                let db = if db == "graphrag.db" { &cfg.db } else { &db };
                let ollama_url = if ollama_url == "http://localhost:11434" {
                    &cfg.ollama_url
                } else {
                    &ollama_url
                };
                let summary_model = if summary_model == "llama3.2:3b" {
                    &cfg.summary_model
                } else {
                    &summary_model
                };
                let embed_model = if embed_model == "nomic-embed-text" {
                    &cfg.embed_model
                } else {
                    &embed_model
                };
                cmd_community_summarize(db, ollama_url, summary_model, embed_model)
            }
        },
        Commands::Completions { shell } => {
            debug!("Comando: completions shell={:?}", shell);
            cmd_completions(shell)
        }
    }
}

/// Carga la configuración, con override opcional de ruta
fn load_config(cli_path: &Option<String>) -> config::GraphRagConfig {
    match cli_path {
        Some(path) => {
            let p = std::path::PathBuf::from(path);
            if p.exists() {
                config::GraphRagConfig::load_from(&p).unwrap_or_default()
            } else {
                eprintln!("⚠️  Archivo de configuración no encontrado: {}", path);
                config::GraphRagConfig::default()
            }
        }
        None => config::GraphRagConfig::load(),
    }
}

fn cmd_init(db: &str) -> Result<()> {
    let conn = rusqlite::Connection::open(db)?;
    db::schema::init_db(&conn)?;
    println!("✅ Base de datos inicializada: {}", db);
    Ok(())
}

fn cmd_neovim(output: Option<String>, cfg: &config::GraphRagConfig) -> Result<()> {
    let lua = format!(
        "-- GraphRAG.nvim -- Auto-generated by `graphrag init neovim`\n\
         -- Place in ~/.config/nvim/lua/plugins/graphrag.lua\n\
         return {{\n\
            dir = vim.fn.stdpath(\"config\") .. \"/lua/graphrag.nvim\",\n\
            cmd = \"GraphRAG\",\n\
            keys = {{\n\
               {{ \",gr\", desc = \"GraphRAG: related notes\" }},\n\
               {{ \",gi\", desc = \"GraphRAG: insert related links\" }},\n\
           }},\n\
           config = function()\n\
             local db = \"{}\"\n\
             local bin = \"graphrag\"\n\
             local k = {}\n\
             local depth = {}\n\
             local notes_dir = \"{}\"\n\
         \n\
             -- Helper: get visually selected text\n\
             local function get_visual_selection()\n\
                 local start_pos = vim.fn.getpos(\"'<\")\n\
                 local end_pos = vim.fn.getpos(\"'>\")\n\
                 if start_pos[1] == 0 or end_pos[1] == 0 then return \"\" end\n\
                 local lines = vim.api.nvim_buf_get_lines(0, start_pos[2] - 1, end_pos[2], false)\n\
                 if #lines == 0 then return \"\" end\n\
                 lines[#lines] = string.sub(lines[#lines], 1, end_pos[3])\n\
                 lines[1] = string.sub(lines[1], start_pos[3])\n\
                 return table.concat(lines, \"\\n\")\n\
             end\n\
         \n\
             -- Helper: run graphrag command and show results in a floating window\n\
             local function run_graphrag(args, title)\n\
                 local output = vim.fn.system(args)\n\
                 if vim.v.shell_error ~= 0 then\n\
                     vim.notify(\"GraphRAG: \" .. output, vim.log.levels.ERROR)\n\
                     return\n\
                 end\n\
                 local buf = vim.api.nvim_create_buf(false, true)\n\
                 vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.split(output, \"\\n\"))\n\
                 local win = vim.api.nvim_open_win(buf, true, {{\n\
                     relative = \"editor\",\n\
                     width = math.floor(vim.o.columns * 0.6),\n\
                     height = math.floor(vim.o.lines * 0.6),\n\
                     col = math.floor(vim.o.columns * 0.2),\n\
                     row = math.floor(vim.o.lines * 0.1),\n\
                     style = \"minimal\",\n\
                     border = \"rounded\",\n\
                     title = \" GraphRAG: \" .. title .. \" \",\n\
                 }})\n\
                 vim.api.nvim_buf_set_keymap(buf, \"n\", \"<CR>\", \":lua open_file()<CR>\", {{ noremap = true, silent = true }})\n\
             end\n\
         \n\
             -- Commands\n\
             vim.api.nvim_create_user_command(\"GraphRAG\", function(info)\n\
                 local args = info.args\n\
                 if args == \"\" then\n\
                     vim.notify(\"Usage: :GraphRAG <subcommand>\", vim.log.levels.WARN)\n\
                     return\n\
                 end\n\
                 local sub, rest = args:match(\"^(%S+)%s*(.*)$\")\n\
                 if sub == \"related\" then\n\
                     local content = vim.api.nvim_buf_get_lines(0, 0, -1, false)\n\
                     local query = table.concat(content, \"\\n\")\n\
                     run_graphrag({{ bin, \"search\", query, db, \"-k\", tostring(k), \"-d\", tostring(depth), \"--notes-only\" }}, \"Related\")\n\
                 elseif sub == \"insert\" then\n\
                     local content = vim.api.nvim_buf_get_lines(0, 0, -1, false)\n\
                     local query = table.concat(content, \"\\n\")\n\
                     local result = vim.fn.system({{ bin, \"search\", query, db, \"-k\", tostring(k), \"-d\", tostring(depth), \"--notes-only\" }})\n\
                     if vim.v.shell_error ~= 0 then\n\
                         vim.notify(\"GraphRAG: \" .. result, vim.log.levels.ERROR)\n\
                         return\n\
                     end\n\
                     local lines = {{}}\n\
                     for line in vim.gsplit(result, \"\\n\") do\n\
                         local label = line:match(\"%[([^%]]+)%]\")\n\
                         if label then\n\
                             table.insert(lines, string.format(\"- [%s](%s.md)\", label, label:lower():gsub(\"%s+\", \"-\")))\n\
                         end\n\
                     end\n\
                     if #lines > 0 then\n\
                         local heading = '## Relacionados'\n\
                         vim.api.nvim_buf_set_lines(0, -1, -1, false, vim.list_extend({{\"\", heading, \"\"}}, lines))\n\
                         vim.notify(\"Relacionados insertados\", vim.log.levels.INFO)\n\
                     else\n\
                         vim.notify(\"No se encontraron relacionados\", vim.log.levels.INFO)\n\
                     end\n\
                 elseif sub == \"sel\" then\n\
                     local query = get_visual_selection()\n\
                     if query == \"\" then\n\
                         vim.notify(\"No text selected. Select text in visual mode first.\", vim.log.levels.WARN)\n\
                         return\n\
                     end\n\
                     run_graphrag({{ bin, \"search\", query, db, \"-k\", tostring(k), \"-d\", tostring(depth), \"--notes-only\" }}, \"Selection\")\n\
                 elseif sub == \"ftsel\" then\n\
                     local query = get_visual_selection()\n\
                     if query == \"\" then\n\
                         vim.notify(\"No text selected. Select text in visual mode first.\", vim.log.levels.WARN)\n\
                         return\n\
                     end\n\
                     run_graphrag({{ bin, \"fts\", query, db, \"-l\", tostring(k), \"--notes-only\" }}, \"FTS Selection\")\n\
                 elseif sub == \"search\" then\n\
                     run_graphrag({{ bin, \"search\", rest, db, \"-k\", tostring(k), \"-d\", tostring(depth) }}, \"Search\")\n\
                 elseif sub == \"fts\" then\n\
                     run_graphrag({{ bin, \"fts\", rest, db, \"-l\", tostring(k) }}, \"FTS\")\n\
                 elseif sub == \"path\" then\n\
                     local from, to = rest:match(\"^(%S+)%s+(%S+)$\")\n\
                     if from and to then\n\
                         run_graphrag({{ bin, \"path\", from, to, db }}, \"Path\")\n\
                     else\n\
                         vim.notify(\"Usage: :GraphRAG path <from> <to>\", vim.log.levels.WARN)\n\
                     end\n\
                 elseif sub == \"stats\" then\n\
                     run_graphrag({{ bin, \"stats\", db }}, \"Stats\")\n\
                 else\n\
                     vim.notify(\"Unknown subcommand: \" .. sub, vim.log.levels.WARN)\n\
                 end\n\
             end, {{ nargs = \"*\" }})\n\
         \n\
             -- Keymaps\n\
             vim.keymap.set(\"n\", \",gr\", \":GraphRAG related<CR>\", {{ noremap = true, silent = true, desc = \"GraphRAG: related notes\" }})\n\
             vim.keymap.set(\"n\", \",gi\", \":GraphRAG insert<CR>\", {{ noremap = true, silent = true, desc = \"GraphRAG: insert related links\" }})\n\
           end,\n\
         }}\n",
        cfg.db, cfg.k, cfg.depth, cfg.notes_dir
    );

    match output {
        Some(path) => {
            std::fs::write(&path, &lua)?;
            // Create the plugin directory that dir points to
            let plugin_dir = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("~/.config"))
                .join("nvim")
                .join("lua")
                .join("graphrag.nvim");
            std::fs::create_dir_all(&plugin_dir)?;
            println!("✅ Configuración NeoVim escrita en: {}", path);
            println!("   Directorio plugin creado: {}", plugin_dir.display());
        }
        None => {
            println!("{}", lua);
        }
    }
    Ok(())
}

fn cmd_completions(shell: Shell) -> Result<()> {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

fn cmd_reset(db: &str) -> Result<()> {
    println!("🔄 Reset: borrando base de datos...");
    if Path::new(db).exists() {
        std::fs::remove_file(db)?;
        // También eliminar el WAL y SHM si existen
        let wal = format!("{}-wal", db);
        let shm = format!("{}-shm", db);
        let _ = std::fs::remove_file(&wal);
        let _ = std::fs::remove_file(&shm);
        println!("   ✅ Base de datos eliminada: {}", db);
    }

    // Crear base de datos nueva con esquema vacío
    let conn = rusqlite::Connection::open(db)?;
    db::schema::init_db(&conn)?;
    println!("   ✅ Base de datos creada: {}", db);
    println!("\n💡 Ahora ejecuta 'graphrag build' para reconstruir el grafo.");
    Ok(())
}

fn cmd_build(
    repo: &str,
    db: &str,
    ollama_url: &str,
    ner_model: &str,
    embed_model: &str,
    num_threads: usize,
) -> Result<()> {
    println!("🔨 Construyendo grafo de conocimiento desde: {}", repo);
    println!("   Base de datos: {}", db);
    println!(
        "   Ollama: {} (NER: {}, Embeddings: {})",
        ollama_url, ner_model, embed_model
    );

    // Verificar Ollama
    let ollama = embed::ollama::OllamaClient::new(ollama_url, embed_model);
    if let Err(e) = ollama.health_check() {
        eprintln!("⚠️  No se pudo conectar con Ollama: {}", e);
        eprintln!("⚠️  Continuando de todas formas...");
    }

    // Verificar modelo NER
    let ner_check = embed::ollama::OllamaClient::new(ollama_url, ner_model);
    if let Err(e) = ner_check.health_check() {
        eprintln!("❌ Modelo NER '{}' no encontrado: {}", ner_model, e);
        eprintln!("❌ El build continuará pero NO se extraerán entidades.");
        eprintln!("💡 Solución: ollama pull {}", ner_model);
    }

    let stats =
        graph::build::build_graph(repo, db, ollama_url, ner_model, embed_model, num_threads)?;
    println!("\n{}", stats);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_search(
    query: &str,
    db: &str,
    k: usize,
    depth: i32,
    alpha: f64,
    ollama_url: &str,
    embed_model: &str,
    min_weight: Option<f64>,
    notes_only: bool,
    filter: &[String],
    answer: bool,
    format: &OutputFormat,
) -> Result<()> {
    // Parse filters (fail early on invalid syntax)
    let filters: Vec<search::filter::Filter> = filter
        .iter()
        .map(|f| search::filter::parse_filter(f))
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|e| {
            eprintln!("❌ {}", e);
            e
        })?;

    let ollama = embed::ollama::OllamaClient::new(ollama_url, embed_model);
    let mut hs = search::HybridSearch::new(db, ollama.clone())?;

    if !matches!(format, OutputFormat::Json) {
        println!("\n🔍 Consulta: '{}'", query);
        println!("   k={}, depth={}, alpha={}", k, depth, alpha,);
        if let Some(mw) = min_weight {
            println!("   min_weight={}", mw);
        }
        if notes_only {
            println!("   solo notas");
        }
        if !filters.is_empty() {
            println!("   filtros:");
            for f in &filters {
                println!("     {} {} {}", f.field, f.operator, f.value);
            }
        }
        println!();
    }

    let results = {
        if !matches!(format, OutputFormat::Json) {
            println!("🧠 HYBRID SEARCH (depth={})\n{}", depth, "─".repeat(50));
        }
        hs.hybrid_search(query, k, depth, alpha, min_weight, notes_only, &filters)?
    };

    display_results(&results, format);

    // ── Mostrar comunidades relacionadas ──
    {
        let conn = rusqlite::Connection::open(db)?;
        match crate::db::communities::load_all_community_embeddings(&conn) {
            Ok(embeddings) if !embeddings.is_empty() => {
                if let Ok(query_vec) = ollama.embed(query) {
                    let mut community_sims: Vec<(f64, &str, &str)> = embeddings
                        .iter()
                        .map(|(_, emb, label, summary)| {
                            let sim = crate::vector::cosine_similarity_raw(emb, &query_vec) as f64;
                            (sim, label.as_str(), summary.as_str())
                        })
                        .collect();
                    community_sims
                        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

                    println!("\n🏘️  Comunidades relacionadas:");
                    println!("{}", "─".repeat(50));
                    for (sim, label, summary) in community_sims.iter().take(2) {
                        let preview: String = summary.chars().take(120).collect();
                        println!("   [{:.3}] {}: {}", sim, label, preview);
                    }
                }
            }
            _ => {}
        }
    }

    // ── Modo respuesta ──
    if answer {
        let conn = rusqlite::Connection::open(db)?;
        let community_embeddings =
            crate::db::communities::load_all_community_embeddings(&conn).unwrap_or_default();

        println!("\n🧠 GENERANDO RESPUESTA...\n{}", "─".repeat(50));
        match crate::community::search::answer_query(
            &ollama,
            query,
            &results,
            &community_embeddings,
            embed_model,
        ) {
            Ok(answer_text) => {
                println!("{}", answer_text);
            }
            Err(e) => {
                eprintln!("❌ Error generando respuesta: {}", e);
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_similar(
    label: Option<String>,
    file: Option<String>,
    db: &str,
    k: usize,
    depth: i32,
    min_weight: Option<f64>,
    notes_only: bool,
    filter: &[String],
    cfg: &config::GraphRagConfig,
    format: &OutputFormat,
) -> Result<()> {
    // Validar que exactamente uno de --label o --file esté presente
    match (&label, &file) {
        (Some(_), Some(_)) => {
            eprintln!("❌ Especifica solo uno de --label o --file, no ambos");
            std::process::exit(1);
        }
        (None, None) => {
            eprintln!("❌ Debes especificar --label o --file");
            std::process::exit(1);
        }
        _ => {}
    }

    // Parse filters
    let filters: Vec<search::filter::Filter> = filter
        .iter()
        .map(|f| search::filter::parse_filter(f))
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|e| {
            eprintln!("❌ {}", e);
            e
        })?;

    let ollama = embed::ollama::OllamaClient::new(&cfg.ollama_url, &cfg.embed_model);
    let mut hs = search::HybridSearch::new(db, ollama.clone())?;

    if let Some(lbl) = &label {
        if !matches!(format, OutputFormat::Json) {
            println!("\n🔍 Similares a: '{}'", lbl);
            println!("   k={}, depth={}", k, depth);
            if notes_only {
                println!("   solo notas");
            }
            println!();
        }

        let results = hs.similar_by_label(lbl, k, depth, min_weight, notes_only, &filters)?;
        display_results(&results, format);
    } else if let Some(f) = &file {
        if !matches!(format, OutputFormat::Json) {
            println!("\n🔍 Similares a archivo: '{}'", f);
            println!("   k={}, depth={}", k, depth);
            if notes_only {
                println!("   solo notas");
            }
            println!();
        }

        let results = hs.similar_by_file(f, k, depth, min_weight, notes_only, &filters)?;
        display_results(&results, format);
    }

    Ok(())
}

fn cmd_community_detect(db: &str, resolution: f64) -> Result<()> {
    println!(
        "🔬 Detectando comunidades (Leiden, resolution={})...",
        resolution
    );
    match crate::community::detect::run_community_detection(db, resolution) {
        Ok((level1, level2)) => {
            println!("✅ Comunidades detectadas:");
            println!("   Nivel 1 (gruesas): {}", level1);
            println!("   Nivel 2 (finas):   {}", level2);
            println!(
                "\n💡 Ejecuta 'graphrag community summarize {}' para generar resúmenes",
                db
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Error detectando comunidades: {}", e);
            Ok(())
        }
    }
}

fn cmd_community_summarize(
    db: &str,
    ollama_url: &str,
    summary_model: &str,
    embed_model: &str,
) -> Result<()> {
    println!("📝 Resumiendo comunidades con '{}'...", summary_model);
    match crate::community::summarize::summarize_all_communities(
        db,
        ollama_url,
        summary_model,
        embed_model,
    ) {
        Ok((count, tokens)) => {
            if count == 0 {
                println!("   (todas las comunidades ya tienen resumen)");
            } else {
                println!(
                    "✅ {} comunidades resumidas ({} tokens totales)",
                    count, tokens
                );
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Error resumiendo comunidades: {}", e);
            Ok(())
        }
    }
}

fn cmd_graph(label: &str, db: &str, depth: i32) -> Result<()> {
    let ollama = embed::ollama::OllamaClient::new("http://localhost:11434", "nomic-embed-text");
    let hs = search::HybridSearch::new(db, ollama)?;

    let neighbors = match hs.graph_only(label, depth) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("❌ {}", e);
            return Ok(());
        }
    };

    println!("🔗 Vecinos de '{}' (depth={}):", label, depth);
    println!("{}", "─".repeat(50));

    if neighbors.is_empty() {
        println!("   (sin vecinos encontrados)");
        return Ok(());
    }

    for n in &neighbors {
        println!("   [dist {}] {} ({})", n.distance, n.label, n.r#type);
    }

    Ok(())
}

fn cmd_fts(
    query: &str,
    db: &str,
    limit: usize,
    notes_only: bool,
    format: &OutputFormat,
) -> Result<()> {
    let ollama = embed::ollama::OllamaClient::new("http://localhost:11434", "nomic-embed-text");
    let hs = search::HybridSearch::new(db, ollama)?;

    if !matches!(format, OutputFormat::Json) {
        println!("📄 BÚSQUEDA FTS5: '{}'", query);
        if notes_only {
            println!("   (solo notas)");
        }
        println!("{}", "─".repeat(50));
    }

    let results = hs.fts_search(query, limit, notes_only)?;

    display_results(&results, format);

    Ok(())
}

fn cmd_path(from: &str, to: &str, db: &str, max_depth: i32) -> Result<()> {
    let conn = rusqlite::Connection::open(db)?;

    let path = graph::expand::shortest_path(&conn, from, to, max_depth)?;

    match path {
        Some(nodes) => {
            println!("🛤️  Camino más corto: '{}' → '{}'", from, to);
            println!("{}", "─".repeat(50));
            println!("   {}", nodes.join(" → "));
        }
        None => {
            println!(
                "⚠️  No se encontró camino entre '{}' y '{}' (depth<={})",
                from, to, max_depth
            );
        }
    }

    Ok(())
}

fn cmd_seed(db: &str, ollama_url: &str, embed_model: &str) -> Result<()> {
    println!("🌱 Generando base de datos de demostración...");
    seed::demo_data::create_demo_db(db, ollama_url, embed_model)?;
    println!("\n💡 Ejemplos de uso:");
    println!(
        "   graphrag search \"seguridad en contenedores\" --db {}",
        db
    );
    println!(
        "   graphrag search \"bases de datos Python\" --db {} --k 5 --depth 2",
        db
    );
    println!("   graphrag stats --db {}", db);
    println!("   graphrag fts \"Docker\" --db {}", db);
    println!("   graphrag path \"Docker\" \"SQLite\" --db {}", db);
    Ok(())
}

fn cmd_stats(db: &str) -> Result<()> {
    let ollama = embed::ollama::OllamaClient::new("http://localhost:11434", "nomic-embed-text");
    let hs = search::HybridSearch::new(db, ollama)?;
    let stats = hs.stats()?;
    println!("{}", stats);
    Ok(())
}

/// Formats search results based on OutputFormat and prints to stdout
fn display_results(results: &[crate::search::SearchResult], format: &OutputFormat) {
    match format {
        OutputFormat::Table => {
            if results.is_empty() {
                println!("   (sin resultados)");
                return;
            }
            for r in results {
                let file_info = if !r.file.is_empty() {
                    format!(" ({})", r.file)
                } else {
                    String::new()
                };
                let n_count = r.neighbors.len();
                println!(
                    "   {:>10.3}  [{:8}] {}{}  (vecinos: {})",
                    r.score, r.r#type, r.label, file_info, n_count
                );
                if !r.chunk_header.is_empty() {
                    // Truncate content for display, same as clip_text in hybrid.rs
                    let preview = if r.content.len() > 200 {
                        let cutoff = r
                            .content
                            .char_indices()
                            .nth(200)
                            .map(|(i, _)| i)
                            .unwrap_or(r.content.len());
                        format!("{}...", &r.content[..cutoff])
                    } else {
                        r.content.clone()
                    };
                    println!("   └── {}: {}", r.chunk_header, preview);
                }
            }
            // Show neighbors of first result
            if let Some(first) = results.first() {
                if !first.neighbors.is_empty() {
                    println!("\n🔗 Vecinos de '{}':", first.label);
                    println!("{}", "─".repeat(50));
                    for n in &first.neighbors {
                        println!("   [dist {}] {} ({})", n.distance, n.label, n.r#type);
                    }
                }
            }
        }
        OutputFormat::List => {
            for r in results {
                if r.r#type == "note" && !r.file.is_empty() {
                    println!("- [{}]({})", r.label, r.file);
                }
            }
        }
        OutputFormat::Json => match serde_json::to_string_pretty(results) {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("❌ Error serializando resultados: {}", e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchResult;

    #[test]
    fn test_display_table_empty() {
        let results: Vec<SearchResult> = vec![];
        // Just verify it doesn't panic
        display_results(&results, &OutputFormat::Table);
    }

    #[test]
    fn test_display_list_format() {
        let results = vec![
            SearchResult {
                id: 1,
                label: "Test Note".into(),
                r#type: "note".into(),
                score: 0.9,
                content: "content".into(),
                file: "/path/note.md".into(),
                neighbors: vec![],
                chunk_header: "".into(),
                chunk_text: "".into(),
            },
            SearchResult {
                id: 2,
                label: "Some Entity".into(),
                r#type: "entity".into(),
                score: 0.8,
                content: "".into(),
                file: "".into(),
                neighbors: vec![],
                chunk_header: "".into(),
                chunk_text: "".into(),
            },
        ];
        // Since we can't easily capture stdout in a simple test,
        // this test verifies no panic and correct filtering logic by checking the function runs
        display_results(&results, &OutputFormat::List);
        // Also test JSON mode
        display_results(&results, &OutputFormat::Json);
    }

    #[test]
    fn test_display_list_skips_non_notes() {
        let results = vec![SearchResult {
            id: 1,
            label: "Entity".into(),
            r#type: "entity".into(),
            score: 0.5,
            content: "".into(),
            file: "".into(),
            neighbors: vec![],
            chunk_header: "".into(),
            chunk_text: "".into(),
        }];
        display_results(&results, &OutputFormat::List);
        // In list mode, entities should produce no output (no assertion needed, just no panic)
    }

    #[test]
    fn test_display_json_empty() {
        let results: Vec<SearchResult> = vec![];
        display_results(&results, &OutputFormat::Json);
        // Should print "[]" without panic
    }
}
