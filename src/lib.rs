use anyhow::{Context, Result};
use rayon::prelude::*;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

pub struct AppContext {
    term: Arc<AtomicBool>,
    chunk_size: usize,
    extensions: Option<Vec<OsString>>,
    input_path: PathBuf,
    script: Option<PathBuf>,
}

impl AppContext {
    pub fn new(
        chunk_size: usize,
        extensions: Option<Vec<OsString>>,
        input_path: PathBuf,
        script: Option<PathBuf>,
    ) -> Result<Self> {
        let term = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(SIGTERM, Arc::clone(&term))?;
        signal_hook::flag::register(SIGINT, Arc::clone(&term))?;

        Ok(Self {
            term,
            chunk_size,
            extensions,
            input_path,
            script,
        })
    }

    pub fn should_term(&self) -> bool {
        should_term(&self.term)
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }
}

/// Executes a command in parallel for a given chunk of file paths.
///
/// This function processes a chunk of file paths in parallel, executing the specified command
/// for each file. It uses Rayon's parallel iterator to distribute the workload across multiple threads.
///
/// # Arguments
///
/// * `context` - A reference to the `AppContext` containing processing configuration and state.
/// * `chunk` - A slice of `PathBuf` representing the files to be processed.
///
/// # Returns
///
/// Returns a `Result<(usize, usize)>` indicating success or failure of the operation.
///
/// The tuple contains the number of files successfully processed and the number of files that encountered errors during
/// processing.
///
/// # Errors
///
/// This function may return an error if there are issues executing the command for any file
/// in the chunk. Errors are propagated from the command execution.
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// let context = AppContext::new(10, None, PathBuf::from("/path/to/input"), Some(PathBuf::from("~/process_file.sh")))?;
/// let chunk = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
/// let (num_processed, num_errored) = parallelize_chunk(&context, &chunk).expect("Failed to process chunk");
/// println!("Processed: {}, Errored: {}", num_processed, num_errored);
/// ```
pub fn parallelize_chunk(context: &AppContext, chunk: &[PathBuf]) -> Result<(usize, usize)> {
    let processed = AtomicUsize::new(0);
    let errored = AtomicUsize::new(0);

    chunk.par_iter().try_for_each(|file| -> Result<()> {
        if context.should_term() {
            log::info!("Cancelling task for file: {}", file.display());
            return Ok(());
        }

        match &context.script {
            Some(script_path) => {
                let output = Command::new(script_path)
                    .arg(file)
                    .output()
                    .with_context(|| {
                        format!("Failed to execute script for file: {}", file.display())
                    })?;

                if output.status.success() {
                    log::debug!("Processed file: {}", file.to_string_lossy());
                    log::debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
                    processed.fetch_add(1, Ordering::Relaxed);
                } else {
                    log::error!("Script failed for file: {}", file.to_string_lossy());
                    log::error!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                    errored.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                // Directly log the file name if no script is provided
                log::info!("Would process file: {}", file.display());
                processed.fetch_add(1, Ordering::Relaxed);
            }
        }

        Ok(())
    })?;

    Ok((
        processed.load(Ordering::Relaxed),
        errored.load(Ordering::Relaxed),
    ))
}

/// Recursively retrieves files from a given directory, optionally filtering by file extensions.
///
/// This function traverses the directory structure starting from the provided `input_path`,
/// collecting file paths that match the specified criteria.
///
/// # Arguments
///
/// * `input_path` - A reference to the `Path` representing the starting directory.
/// * `extensions` - An optional `Vec<&OsStr>` containing file extensions to filter by.
///                  If `None`, all files are included.
///
/// # Returns
///
/// Returns a `Result` containing a `Vec<PathBuf>` of matching file paths.
///
/// # Errors
///
/// This function may return an error if there are issues with file system operations
/// or directory traversal.
pub fn get_files(context: &AppContext) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let should_include = |file_path: &Path| -> bool {
        if let Some(exts) = &context.extensions {
            file_path
                .extension()
                .map(|ext| exts.iter().any(|e| e == ext))
                .unwrap_or(false)
        } else {
            true
        }
    };

    // Check if the input path exists before walking
    if !context.input_path.exists() {
        return Err(anyhow::anyhow!("Input path does not exist"));
    }

    // TODO: parallelize this with rayon!
    for entry in WalkDir::new(&context.input_path).into_iter() {
        let entry = entry
            .with_context(|| format!("Failed to read entry in {}", context.input_path.display()))?;
        if entry.file_type().is_file() && should_include(entry.path()) {
            files.push(entry.path().to_path_buf());
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    fn create_test_directory() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create some test files and directories
        fs::create_dir(base_path.join("subdir")).unwrap();
        File::create(base_path.join("file1.txt")).unwrap();
        File::create(base_path.join("file2.jpg")).unwrap();
        File::create(base_path.join("file3.png")).unwrap();
        File::create(base_path.join("subdir").join("file4.txt")).unwrap();
        File::create(base_path.join("subdir").join("file5.jpg")).unwrap();

        temp_dir
    }

    fn create_test_context(temp_dir: &TempDir, extensions: Option<Vec<OsString>>) -> AppContext {
        AppContext {
            term: Arc::new(AtomicBool::new(false)),
            chunk_size: 10, // Some default value
            extensions,
            input_path: temp_dir.path().to_path_buf(),
            script: None,
        }
    }

    #[test]
    fn test_get_files3_with_extensions() {
        let temp_dir = create_test_directory();
        let extensions = Some(vec![OsString::from("txt"), OsString::from("jpg")]);

        let context = create_test_context(&temp_dir, extensions);
        let files = get_files(&context).unwrap();

        assert_eq!(files.len(), 4);
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file1.txt"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file2.jpg"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file4.txt"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file5.jpg"));
    }

    #[test]
    fn test_get_files3_without_extensions() {
        let temp_dir = create_test_directory();
        let context = create_test_context(&temp_dir, None);

        let files = get_files(&context).unwrap();

        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files3_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir, None);
        let files = get_files(&context).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn test_get_files3_non_existent_directory() {
        let non_existent_path = PathBuf::from("/this/path/does/not/exist");
        let context = AppContext {
            term: Arc::new(AtomicBool::new(false)),
            chunk_size: 10,
            extensions: None,
            input_path: non_existent_path,
            script: None,
        };

        let result = get_files(&context);

        assert!(result.is_err());
    }

    // Additional tests can be added here, such as testing for permission errors,
    // or more complex directory structures.
}

/// Checks if a termination signal has been received.
///
/// This function checks the state of an atomic boolean flag to determine
/// if a termination signal has been received.
///
/// # Arguments
///
/// * `term` - A reference to an `Arc<AtomicBool>` representing the termination flag.
///
/// # Returns
///
/// Returns `true` if a termination signal has been received, `false` otherwise.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::AtomicBool;
///
/// let term_flag = Arc::new(AtomicBool::new(false));
/// if should_term(&term_flag) {
///     println!("Termination signal received");
/// }
/// ```
pub fn should_term(term: &Arc<AtomicBool>) -> bool {
    if term.load(Ordering::Relaxed) {
        log::info!("PFP: CAUGHT SIGNAL! K Thx Bye!");
        return true;
    }
    false
}
