use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SecurityError {
    PathTraversalAttempt,
    ResourceExhaustion,
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityError::PathTraversalAttempt => write!(f, "Path Traversal Attempt Detected"),
            SecurityError::ResourceExhaustion => write!(f, "Resource Exhaustion Attempt Detected"),
        }
    }
}

impl std::error::Error for SecurityError {}

pub struct PathJailer {
    pub root: PathBuf,
}

impl PathJailer {
    #[must_use] 
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn safe_join(&self, guest_path: &str) -> Result<PathBuf, SecurityError> {
        if guest_path.contains("..") || guest_path.contains('\0') {
            return Err(SecurityError::PathTraversalAttempt);
        }

        let full_path = self.root.join(guest_path);
        
        let path_to_eval = if full_path.exists() {
            full_path.canonicalize().unwrap_or(full_path.clone())
        } else {
            let parent = full_path.parent().unwrap_or(Path::new(""));
            if parent.exists() {
                parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()).join(full_path.file_name().unwrap_or_default())
            } else {
                full_path.clone()
            }
        };

        let canon_root = if self.root.exists() {
            self.root.canonicalize().unwrap_or_else(|_| self.root.clone())
        } else {
            self.root.clone()
        };

        if path_to_eval.starts_with(&canon_root) {
            Ok(path_to_eval)
        } else {
            Err(SecurityError::PathTraversalAttempt)
        }
    }
}

pub struct SovereignSandbox {
    pub jailer: PathJailer,
    pub fuel_limit: u64,
}

impl SovereignSandbox {
    #[must_use] 
    pub fn new(worktree_root: PathBuf, fuel_limit: u64) -> Self {
        Self {
            jailer: PathJailer::new(worktree_root),
            fuel_limit,
        }
    }
    
    #[must_use] 
    pub fn engine_config() -> wasmtime::Config {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config
    }

    pub fn resolve_guest_path(&self, guest_path: &str) -> Result<PathBuf, SecurityError> {
        self.jailer.safe_join(guest_path)
    }
}
