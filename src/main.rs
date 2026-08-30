mod db;
mod embed;
mod vector;
mod chunking;
mod ner;
mod graph;
mod search;
mod seed;
mod mcp;
mod config;

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use anyhow::Result;
use std::path::Path;
use log::debug;

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
    /// Búsqueda híbrida (vectores + grafos)
    Search {
        /// Consulta de búsqueda
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
        /// Usar Ollama para embeddings (si no, sintéticos)
        #[arg(long)]
        ollama: bool,
        /// URL de Ollama
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        /// Modelo de embeddings
        #[arg(long, default_value = "nomic-embed-text")]
        embed_model: String,
        /// Solo vectorial (sin grafo)
        #[arg(long)]
        vector_only: bool,
        /// Peso mínimo de arista para expansión
        #[arg(long)]
        min_weight: Option<f64>,
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
    /// Búsqueda textual exacta (FTS5)
    Fts {
        /// Consulta de búsqueda
        query: String,
        /// Ruta a la base de datos
        #[arg(default_value = "graphrag.db")]
        db: String,
        /// Número de resultados
        #[arg(short, long, default_value = "10")]
        limit: usize,
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
    /// Genera scripts de autocompletado para el shell
    Completions {
        /// Shell para el que generar el script
        shell: Shell,
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
    debug!("Config loaded: db={}, ollama_url={}, embed_model={}, ner_model={}",
           cfg.db, cfg.ollama_url, cfg.embed_model, cfg.ner_model);

    match cli.command {
        Commands::Init { action } => match action {
            InitCommands::Db { db } => {
                let db = if db == "graphrag.db" { &cfg.db } else { &db };
                cmd_init(db)
            }
            InitCommands::Neovim { output } => cmd_neovim(output, &cfg),
        },
        Commands::Build { repo, db, embed_model, ner_model, ollama_url } => {
            let db = if db == "graphrag.db" { cfg.db } else { db };
            let repo = if repo == "." && !cfg.notes_dir.is_empty() { cfg.notes_dir.clone() } else { repo };
            let embed_model = if embed_model == "nomic-embed-text" { cfg.embed_model } else { embed_model };
            let ner_model = if ner_model == "llama3.2:3b" { cfg.ner_model } else { ner_model };
            let ollama_url = if ollama_url == "http://localhost:11434" { cfg.ollama_url } else { ollama_url };
            debug!("Comando: build repo={}, db={}", repo, db);
            cmd_build(&repo, &db, &ollama_url, &ner_model, &embed_model, cfg.num_threads)
        }
        Commands::Search { query, db, k, depth, alpha, ollama, ollama_url, embed_model, vector_only, min_weight } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let k = if k == 5 { cfg.k } else { k };
            let depth = if depth == 2 { cfg.depth } else { depth };
            let alpha_val = if (alpha - 0.7).abs() < 0.01 { cfg.alpha } else { alpha };
            let ollama_url = if ollama_url == "http://localhost:11434" { &cfg.ollama_url } else { &ollama_url };
            let embed_model = if embed_model == "nomic-embed-text" { &cfg.embed_model } else { &embed_model };
            debug!("Comando: search query='{}', k={}, depth={}", query, k, depth);
            cmd_search(&query, db, k, depth, alpha_val, ollama, ollama_url, embed_model, vector_only, min_weight)
        }
        Commands::Graph { label, db, depth } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let depth = if depth == 2 { cfg.depth } else { depth };
            debug!("Comando: graph label='{}', depth={}", label, depth);
            cmd_graph(&label, db, depth)
        },
        Commands::Fts { query, db, limit } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: fts query='{}', limit={}", query, limit);
            cmd_fts(&query, db, limit)
        },
        Commands::Path { from, to, db, max_depth } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: path from='{}', to='{}'", from, to);
            cmd_path(&from, &to, db, max_depth)
        },
        Commands::Seed { db } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: seed db={}", db);
            cmd_seed(db)
        }
        Commands::Reset { db } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            debug!("Comando: reset db={}", db);
            cmd_reset(db)
        }
        Commands::Mcp { db, ollama_url, embed_model } => {
            let db = if db == "graphrag.db" { &cfg.db } else { &db };
            let ollama_url = if ollama_url == "http://localhost:11434" { &cfg.ollama_url } else { &ollama_url };
            let embed_model = if embed_model == "nomic-embed-text" { &cfg.embed_model } else { &embed_model };
            debug!("Comando: mcp db={}", db);
            mcp::run_server(db, ollama_url, embed_model, cfg.num_threads)
        }
        Commands::Stats { db } => {
            debug!("Comando: stats db={}", db.as_deref().unwrap_or(&cfg.db));
            cmd_stats(db.as_deref().unwrap_or(&cfg.db))
        }
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
         -- Install with lazy.nvim:\n\
         --   {{ \"graphrag.nvim\", config = true }}\n\
         \n\
         local M = {{}}\n\
         \n\
         function M.setup(opts)\n\
             opts = opts or {{}}\n\
             local db = opts.db or \"{}\"\n\
             local bin = opts.bin or \"graphrag\"\n\
             local k = opts.k or {}\n\
             local depth = opts.depth or {}\n\
             local notes_dir = opts.notes_dir or \"{}\"\n\
         \n\
             -- Helper: run graphrag command and show results in a floating window\n\
             local function run_graphrag(cmd, title)\n\
                 local output = vim.fn.system({{ bin }} .. \" \" .. cmd)\n\
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
             -- GraphRAG related: search notes related to current buffer\n\
             vim.api.nvim_create_user_command(\"GraphRAG\", function(info)\n\
                 local args = info.args\n\
                 if args == \"\" then\n\
                     vim.notify(\"Usage: :GraphRAG <subcommand>\", vim.log.levels.WARN)\n\
                     return\n\
                 end\n\
                 local sub, rest = args:match(\"^(%S+)%s*(.*)$\")\n\
                 if sub == \"related\" then\n\
                     local title = vim.fn.expand(\"%:t:r\")\n\
                     run_graphrag(string.format('search \"%s\" --db %s -k %d -d %d', title, db, k, depth), \"Related: \" .. title)\n\
                 elseif sub == \"insert\" then\n\
                     local title = vim.fn.expand(\"%:t:r\")\n\
                     local result = vim.fn.system({{ bin, \"search\", title, \"--db\", db, \"-k\", tostring(k), \"-d\", tostring(depth) }})\n\
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
                 elseif sub == \"search\" then\n\
                     run_graphrag(string.format('search \"%s\" --db %s -k %d -d %d', rest, db, k, depth), \"Search\")\n\
                 elseif sub == \"fts\" then\n\
                     run_graphrag(string.format('fts \"%s\" --db %s -l %d', rest, db, k), \"FTS\")\n\
                 elseif sub == \"path\" then\n\
                     local from, to = rest:match(\"^(%S+)%s+(%S+)$\")\n\
                     if from and to then\n\
                         run_graphrag(string.format('path \"%s\" \"%s\" --db %s', from, to, db), \"Path\")\n\
                     else\n\
                         vim.notify(\"Usage: :GraphRAG path <from> <to>\", vim.log.levels.WARN)\n\
                     end\n\
                 elseif sub == \"stats\" then\n\
                     run_graphrag(string.format(\"stats --db %s\", db), \"Stats\")\n\
                 else\n\
                     vim.notify(\"Unknown subcommand: \" .. sub, vim.log.levels.WARN)\n\
                 end\n\
             end, {{ nargs = \"*\" }})\n\
         \n\
             -- Keymaps\n\
             vim.keymap.set(\"n\", \",gr\", \":GraphRAG related<CR>\", {{ noremap = true, silent = true, desc = \"GraphRAG: related notes\" }})\n\
             vim.keymap.set(\"n\", \",gi\", \":GraphRAG insert<CR>\", {{ noremap = true, silent = true, desc = \"GraphRAG: insert related links\" }})\n\
         end\n\
         \n\
         return M\n",
        cfg.db, cfg.k, cfg.depth, cfg.notes_dir
    );

    match output {
        Some(path) => {
            std::fs::write(&path, &lua)?;
            println!("✅ Configuración NeoVim escrita en: {}", path);
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

fn cmd_build(repo: &str, db: &str, ollama_url: &str, ner_model: &str, embed_model: &str, num_threads: usize) -> Result<()> {
    println!("🔨 Construyendo grafo de conocimiento desde: {}", repo);
    println!("   Base de datos: {}", db);
    println!("   Ollama: {} (NER: {}, Embeddings: {})", ollama_url, ner_model, embed_model);

    // Verificar Ollama
    let ollama = embed::ollama::OllamaClient::new(ollama_url, embed_model);
    if let Err(e) = ollama.health_check() {
        eprintln!("⚠️  No se pudo conectar con Ollama: {}", e);
        eprintln!("⚠️  Continuando de todas formas...");
    }

    // Verificar modelo NER
    let ner_check = embed::ollama::OllamaClient::new(ollama_url, ner_model);
    if let Err(e) = ner_check.health_check() {
        eprintln!("⚠️  Modelo NER '{}' no encontrado: {}", ner_model, e);
        eprintln!("⚠️  El build continuará pero NO se extraerán entidades.");
        eprintln!("💡  Para extraer entidades: ollama pull {}", ner_model);
    }

    let stats = graph::build::build_graph(repo, db, ollama_url, ner_model, embed_model, num_threads)?;
    println!("\n{}", stats);
    Ok(())
}

fn cmd_search(
    query: &str, db: &str, k: usize, depth: i32, alpha: f64,
    use_ollama: bool, ollama_url: &str, embed_model: &str,
    vector_only: bool, min_weight: Option<f64>,
) -> Result<()> {
    let ollama = if use_ollama {
        Some(embed::ollama::OllamaClient::new(ollama_url, embed_model))
    } else {
        None
    };

    let mut hs = search::HybridSearch::new(db, ollama)?;

    println!("\n🔍 Consulta: '{}'", query);
    println!("   k={}, depth={}, alpha={}, embed={}",
             k, depth, alpha, if use_ollama { "Ollama" } else { "sintético" });
    if let Some(mw) = min_weight {
        println!("   min_weight={}", mw);
    }
    println!();

    let results = if vector_only {
        println!("📊 RAG VECTORIAL PURO (depth=0)\n{}", "─".repeat(50));
        hs.vector_only(query, k)?
    } else {
        println!("🧠 HYBRID SEARCH (depth={})\n{}", depth, "─".repeat(50));
        hs.hybrid_search(query, k, depth, alpha, min_weight)?
    };

    if results.is_empty() {
        println!("   (sin resultados)");
        return Ok(());
    }

    for r in &results {
        let file_info = if !r.file.is_empty() {
            format!(" ({})", r.file)
        } else {
            String::new()
        };
        let n_count = r.neighbors.len();
        println!("   {:>10.3}  [{:8}] {}{}  (vecinos: {})",
                 r.score, r.r#type, r.label, file_info, n_count);
    }

    // Mostrar vecinos del primer resultado
    if let Some(first) = results.first() {
        if !first.neighbors.is_empty() {
            println!("\n🔗 Vecinos de '{}':", first.label);
            println!("{}", "─".repeat(50));
            for n in &first.neighbors {
                println!("   [dist {}] {} ({})", n.distance, n.label, n.r#type);
            }
        }
    }

    Ok(())
}

fn cmd_graph(label: &str, db: &str, depth: i32) -> Result<()> {
    let hs = search::HybridSearch::new(db, None)?;

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

fn cmd_fts(query: &str, db: &str, limit: usize) -> Result<()> {
    let hs = search::HybridSearch::new(db, None)?;

    println!("📄 BÚSQUEDA FTS5: '{}'", query);
    println!("{}", "─".repeat(50));

    let results = hs.fts_search(query, limit)?;

    if results.is_empty() {
        println!("   (sin resultados)");
        return Ok(());
    }

    for r in &results {
        let file_info = if !r.file.is_empty() {
            format!(" ({})", r.file)
        } else {
            String::new()
        };
        println!("   {:>10.3}  [{}] {}{}", r.score, r.r#type, r.label, file_info);
    }

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
            println!("⚠️  No se encontró camino entre '{}' y '{}' (depth<={})",
                     from, to, max_depth);
        }
    }

    Ok(())
}

fn cmd_seed(db: &str) -> Result<()> {
    println!("🌱 Generando base de datos de demostración...");
    seed::demo_data::create_demo_db(db)?;
    println!("\n💡 Ejemplos de uso:");
    println!("   graphrag search \"seguridad en contenedores\" --db {}", db);
    println!("   graphrag search \"bases de datos Python\" --db {} --k 5 --depth 2", db);
    println!("   graphrag stats --db {}", db);
    println!("   graphrag fts \"Docker\" --db {}", db);
    println!("   graphrag path \"Docker\" \"SQLite\" --db {}", db);
    Ok(())
}

fn cmd_stats(db: &str) -> Result<()> {
    let hs = search::HybridSearch::new(db, None)?;
    let stats = hs.stats()?;
    println!("{}", stats);
    Ok(())
}