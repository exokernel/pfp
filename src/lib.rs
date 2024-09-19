use anyhow::{Context, Result};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// Executes a command in parallel for a given chunk of file paths.
///
/// This function processes a chunk of file paths in parallel, executing the specified command
/// for each file. It uses Rayon's parallel iterator to distribute the workload across multiple threads.
///
/// # Arguments
///
/// * `chunk` - A slice of `PathBuf` representing the files to be processed.
/// * `command` - A string slice containing the command to be executed for each file.
/// * `term` - A reference to an `Arc<AtomicBool>` representing the termination flag. Allows tasks to return early if a
///            termination signal is received.
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
/// use std::sync::atomic::AtomicBool;
/// use std::sync::Arc;
/// let chunk = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
/// let script = "~/process_file.sh";
/// let term = Arc::new(AtomicBool::new(false));
/// let (num_processed, num_errored) = parallelize_chunk(&chunk, command, &term).expect("Failed to process chunk");
/// println!("Processed: {}, Errored: {}", num_processed, num_errored);
/// ```
pub fn parallelize_chunk<F>(
    chunk: &[PathBuf],
    script: Option<&Path>,
    should_cancel: F,
) -> Result<(usize, usize)>
where
    F: Fn() -> bool + Send + Sync,
{
    let processed = AtomicUsize::new(0);
    let errored = AtomicUsize::new(0);

    chunk.par_iter().try_for_each(|file| -> Result<()> {
        if should_cancel() {
            log::info!("Cancelling task for file: {}", file.display());
            return Ok(());
        }

        match script {
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
                // Directly print the file name to stdout
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
pub fn get_files(input_path: &Path, extensions: &Option<Vec<&OsStr>>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let should_include = |file_path: &Path| -> bool {
        if let Some(exts) = extensions {
            file_path
                .extension()
                .map(|ext| exts.contains(&ext))
                .unwrap_or(false)
        } else {
            true
        }
    };

    // Check if the input path exists before walking
    if !input_path.exists() {
        return Err(anyhow::anyhow!("Input path does not exist"));
    }

    // TODO: parallelize this with rayon!
    for entry in WalkDir::new(input_path).into_iter() {
        let entry =
            entry.with_context(|| format!("Failed to read entry in {}", input_path.display()))?;
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

    #[test]
    fn test_get_files3_with_extensions() {
        let temp_dir = create_test_directory();
        let extensions = Some(vec![OsStr::new("txt"), OsStr::new("jpg")]);

        let files = get_files(temp_dir.path(), &extensions).unwrap();

        assert_eq!(files.len(), 4);
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file1.txt"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file2.jpg"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file4.txt"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "file5.jpg"));
    }

    #[test]
    fn test_get_files3_without_extensions() {
        let temp_dir = create_test_directory();
        let extensions = None;

        let files = get_files(temp_dir.path(), &extensions).unwrap();

        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files3_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let extensions = None;

        let files = get_files(temp_dir.path(), &extensions).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn test_get_files3_non_existent_directory() {
        let non_existent_path = Path::new("/this/path/does/not/exist");
        let extensions = None;

        let result = get_files(non_existent_path, &extensions);

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
