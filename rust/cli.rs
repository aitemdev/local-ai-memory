use crate::{
    embeddings::{default_model, embed_text, resolve_config},
    extractors::parser_status,
    indexer::{
        add_path_with_collection, delete_collection, init_store, list_collections, reset_store,
        search_with_collection, status,
    },
    paths::memory_home,
    settings::{list_settings, set_settings},
};
use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::{collections::HashMap, path::PathBuf};

#[derive(Parser)]
#[command(name = "mem", version, about = "Local-first personal memory for AI tools")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Add(AddArgs),
    Reindex(AddArgs),
    Search(SearchArgs),
    Ask(SearchArgs),
    Status,
    Reset {
        #[arg(long, help = "Required confirmation flag")]
        yes: bool,
    },
    Parsers,
    Embeddings {
        #[command(subcommand)]
        command: Option<EmbeddingCommand>,
    },
    Serve(ServeArgs),
    Daemon(DaemonArgs),
    Watch { path: PathBuf },
    Tui,
    Collections {
        #[command(subcommand)]
        command: CollectionCommand,
    },
    Export { path: PathBuf },
    Import {
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Args)]
struct DaemonArgs {
    #[command(subcommand)]
    command: Option<DaemonCommand>,
    #[arg(long, default_value_t = 7456, global = true)]
    port: u16,
}

#[derive(Subcommand)]
enum DaemonCommand {
    Start,
    Stop,
    Status,
    Restart,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long)]
    mcp: bool,
    #[arg(long)]
    http: bool,
    #[arg(long, default_value_t = 7456)]
    port: u16,
}

#[derive(Args)]
struct AddArgs {
    path: PathBuf,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long = "base-url")]
    base_url: Option<String>,
    #[arg(long)]
    dimensions: Option<usize>,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    collection: Option<String>,
}

#[derive(Args)]
struct SearchArgs {
    query: Vec<String>,
    #[arg(long, default_value = "low")]
    budget: String,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    collection: Option<String>,
}

#[derive(Subcommand)]
enum CollectionCommand {
    List,
    Delete { name: String },
}

#[derive(Subcommand)]
enum EmbeddingCommand {
    Set(EmbeddingSetArgs),
    Test { text: Vec<String> },
}

#[derive(Args)]
struct EmbeddingSetArgs {
    #[arg(long)]
    provider: String,
    #[arg(long)]
    model: Option<String>,
    #[arg(long = "base-url")]
    base_url: Option<String>,
    #[arg(long)]
    dimensions: Option<usize>,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Status) {
        Command::Init => {
            let base = init_store(None)?;
            println!("Initialized local memory store at {}", base.display());
        }
        Command::Add(args) => {
            if let Some(value) = via_daemon_add(&args)? {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                let results = add_path_with_collection(
                    &args.path,
                    args.force,
                    &embedding_overrides(&args),
                    args.collection.as_deref(),
                    None,
                )?;
                for result in results {
                    println!("{}", serde_json::to_string(&result)?);
                }
            }
        }
        Command::Reindex(args) => {
            let results = add_path_with_collection(
                &args.path,
                true,
                &embedding_overrides(&args),
                args.collection.as_deref(),
                None,
            )?;
            for result in results {
                println!("{}", serde_json::to_string(&result)?);
            }
        }
        Command::Search(args) => run_search(args)?,
        Command::Ask(mut args) => {
            if args.budget == "low" {
                args.budget = "normal".to_string();
            }
            run_search(args)?;
        }
        Command::Status => {
            let value = if crate::client::endpoint().is_some() {
                crate::client::get("/api/status")?
            } else {
                status(None)?
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Reset { yes } => {
            if !yes {
                return Err(anyhow!("Refusing to reset without --yes"));
            }
            let value = if crate::client::endpoint().is_some() {
                crate::client::post("/api/reset", &serde_json::json!({}))?
            } else {
                reset_store(None)?
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Parsers => {
            let value = if crate::client::endpoint().is_some() {
                crate::client::get("/api/parsers")?
            } else {
                parser_status()
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Embeddings { command } => run_embeddings(command)?,
        Command::Serve(args) => run_serve(args)?,
        Command::Daemon(args) => run_daemon(args)?,
        Command::Watch { path } => crate::watcher::watch(&path)?,
        Command::Tui => crate::tui::run()?,
        Command::Collections { command } => match command {
            CollectionCommand::List => {
                let value = list_collections(None)?;
                println!("{}", serde_json::to_string_pretty(&value)?);
            }
            CollectionCommand::Delete { name } => {
                let value = delete_collection(&name, None)?;
                println!("{}", serde_json::to_string_pretty(&value)?);
            }
        },
        Command::Export { path } => {
            let result = crate::backup::export(&path, None)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Import { path, force } => {
            let result = crate::backup::import(&path, None, force)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

fn run_daemon(args: DaemonArgs) -> Result<()> {
    match args.command.unwrap_or(DaemonCommand::Start) {
        DaemonCommand::Start => crate::http::serve_daemon(args.port)?,
        DaemonCommand::Stop => {
            let info = crate::daemon::stop(3000)?;
            println!("{}", json!({ "stopped": true, "pid": info.pid, "port": info.port }));
        }
        DaemonCommand::Status => {
            let value = match crate::daemon::read_info() {
                Some(info) => json!({ "running": true, "pid": info.pid, "port": info.port }),
                None => json!({ "running": false }),
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        DaemonCommand::Restart => {
            if crate::daemon::read_info().is_some() {
                let _ = crate::daemon::stop(3000)?;
            }
            crate::http::serve_daemon(args.port)?;
        }
    }
    Ok(())
}

fn run_serve(args: ServeArgs) -> Result<()> {
    if !args.mcp && !args.http {
        return Err(anyhow!("Pass --mcp or --http"));
    }
    if args.mcp && args.http {
        return Err(anyhow!("Pass only one of --mcp or --http"));
    }
    if args.mcp {
        crate::mcp::serve()?;
    } else {
        crate::http::serve(args.port)?;
    }
    Ok(())
}

fn via_daemon_add(args: &AddArgs) -> Result<Option<serde_json::Value>> {
    if crate::client::endpoint().is_none() {
        return Ok(None);
    }
    let absolute = std::fs::canonicalize(&args.path)?;
    let body = serde_json::json!({
        "paths": [absolute.to_string_lossy()],
        "force": args.force,
    });
    Ok(Some(crate::client::post("/api/add", &body)?))
}

fn run_search(args: SearchArgs) -> Result<()> {
    let query = args.query.join(" ");
    if query.trim().is_empty() {
        return Err(anyhow!("Missing query"));
    }
    let rows: Vec<crate::indexer::SearchResult> = if crate::client::endpoint().is_some() {
        let mut path = format!(
            "/api/search?q={}&budget={}",
            crate::client::url_encode(&query),
            crate::client::url_encode(&args.budget)
        );
        if let Some(limit) = args.limit {
            path.push_str(&format!("&limit={limit}"));
        }
        if let Some(collection) = args.collection.as_deref() {
            path.push_str(&format!("&collection={}", crate::client::url_encode(collection)));
        }
        serde_json::from_value(crate::client::get(&path)?)?
    } else {
        search_with_collection(
            &query,
            &args.budget,
            args.limit,
            args.collection.as_deref(),
            &HashMap::new(),
            None,
        )?
    };
    if args.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if rows.is_empty() {
        println!("No results.");
    } else {
        for (index, row) in rows.iter().enumerate() {
            let snippet = row.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let snippet = if snippet.len() > 260 { &snippet[..260] } else { &snippet };
            println!(
                "{}. {} score={} tokens={} chunk={}",
                index + 1,
                row.citation,
                row.score,
                row.token_count,
                row.chunk_id
            );
            println!("   {snippet}");
            if args.debug {
                println!("   scores={} path={}", row.score_breakdown, row.path);
            }
        }
    }
    Ok(())
}

fn run_embeddings(command: Option<EmbeddingCommand>) -> Result<()> {
    match command {
        None => {
            let config = resolve_config(None, &HashMap::new(), true)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "active": redact_config(&config),
                    "settings": list_settings("embedding.", None)?
                }))?
            );
        }
        Some(EmbeddingCommand::Set(args)) => {
            let model = args.model.unwrap_or_else(|| default_model(&args.provider).to_string());
            let mut values = vec![
                ("embedding.provider", args.provider.clone()),
                ("embedding.default_model", model),
                (
                    "embedding.cloud_enabled",
                    if args.provider == "local" { "false" } else { "true" }.to_string(),
                ),
            ];
            if let Some(base_url) = args.base_url {
                values.push(("embedding.base_url", base_url));
            }
            if let Some(dimensions) = args.dimensions {
                values.push(("embedding.dimensions", dimensions.to_string()));
            }
            set_settings(&values, None)?;
            let config = resolve_config(None, &HashMap::new(), true)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "active": redact_config(&config),
                    "settings": list_settings("embedding.", None)?
                }))?
            );
            println!("Reindex documents after changing provider/model so stored vectors match the active embedding config.");
        }
        Some(EmbeddingCommand::Test { text }) => {
            let text = if text.is_empty() { "hello world".to_string() } else { text.join(" ") };
            let embedding = embed_text(&text, None, &HashMap::new())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "provider": embedding.provider,
                    "model": embedding.model,
                    "dimensions": embedding.dimensions,
                    "preview": embedding.vector.iter().take(8).collect::<Vec<_>>()
                }))?
            );
        }
    }
    Ok(())
}

fn embedding_overrides(args: &AddArgs) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(value) = &args.provider {
        map.insert("provider".to_string(), value.clone());
    }
    if let Some(value) = &args.model {
        map.insert("model".to_string(), value.clone());
    }
    if let Some(value) = &args.base_url {
        map.insert("base_url".to_string(), value.clone());
    }
    if let Some(value) = args.dimensions {
        map.insert("dimensions".to_string(), value.to_string());
    }
    map
}

fn redact_config(config: &crate::embeddings::EmbeddingConfig) -> serde_json::Value {
    json!({
        "provider": config.provider,
        "model": config.model,
        "dimensions": config.dimensions,
        "baseUrl": config.base_url,
        "apiKey": if config.api_key.is_some() { "set" } else { "missing" },
        "store": memory_home()
    })
}
