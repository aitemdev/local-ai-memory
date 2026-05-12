use std::path::PathBuf;

pub fn memory_home() -> PathBuf {
    std::env::var_os("MEM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap().join(".memoria"))
}

pub struct DataPaths {
    pub base: PathBuf,
    pub db: PathBuf,
    pub canonical: PathBuf,
    pub originals: PathBuf,
    pub logs: PathBuf,
}

pub fn data_paths(base: Option<PathBuf>) -> DataPaths {
    let base = base.unwrap_or_else(memory_home);
    DataPaths {
        db: base.join("memory.sqlite"),
        canonical: base.join("canonical"),
        originals: base.join("originals"),
        logs: base.join("logs"),
        base,
    }
}
