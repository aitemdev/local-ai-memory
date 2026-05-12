mod chunker;
mod cli;
mod db;
mod embeddings;
mod extractors;
mod hash;
mod indexer;
mod paths;
mod reranker;
mod settings;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}
