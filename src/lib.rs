use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;

mod context;
pub use context::ProcessingContext;

/// Executes a command in parallel for a given chunk of file paths.
///
/// This function processes a chunk of file paths in parallel, executing the specified command
/// for each file. It uses Rayon's parallel iterator to distribute the workload across multiple threads.
///
/// # Arguments
///
/// * `chunk` - A slice of `PathBuf` representing the files to be processed.
/// * `script` - An `Option<&Path>` containing the path to the script to be executed for each file.
///              If `None`, the function will only log the file names without executing a script.
/// * `should_cancel` - A closure that returns a boolean indicating whether the operation should be cancelled.
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
/// This function may return an error if there are issues executing the script for any file
/// in the chunk. Errors are propagated from the script execution.
///
/// # Example
/// ```
/// use std::path::{Path, PathBuf};
/// use std::sync::Arc;
/// use std::sync::atomic::AtomicBool;
///
/// let chunk = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
/// let script = Some(Path::new("/usr/local/bin/process_file.sh"));
/// let term_flag = Arc::new(AtomicBool::new(false));
/// let should_cancel = || term_flag.load(std::sync::atomic::Ordering::Relaxed);
///
/// let (processed, errored) = parallelize_chunk(&chunk, script, should_cancel)
///     .expect("Failed to process chunk");
/// println!("Processed: {}, Errored: {}", processed, errored);
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
                // Directly log the file name
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

#[cfg(test)]
mod parallelize_chunk_tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path, num_files: usize) -> Vec<PathBuf> {
        (0..num_files)
            .map(|i| {
                let path = dir.join(format!("test_file_{}.txt", i));
                File::create(&path).unwrap();
                path
            })
            .collect()
    }

    #[test]
    fn test_parallelize_chunk_without_script() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path(), 5);
        let term_flag = Arc::new(AtomicBool::new(false));
        let should_cancel = || term_flag.load(Ordering::Relaxed);

        let (processed, errored) = parallelize_chunk(&files, None, should_cancel).unwrap();

        assert_eq!(processed, 5);
        assert_eq!(errored, 0);
    }

    #[test]
    fn test_parallelize_chunk_with_cancellation() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path(), 5);
        let term_flag = Arc::new(AtomicBool::new(true));
        let should_cancel = || term_flag.load(Ordering::Relaxed);

        let (processed, errored) = parallelize_chunk(&files, None, should_cancel).unwrap();

        assert_eq!(processed, 0);
        assert_eq!(errored, 0);
    }

    #[test]
    fn test_parallelize_chunk_with_script() {
        let temp_dir = TempDir::new().unwrap();
        let files = create_test_files(temp_dir.path(), 3);

        let script_content = "#!/bin/bash\necho \"Processing $1\"";
        let script_path = temp_dir.path().join("test_script.sh");
        File::create(&script_path)
            .unwrap()
            .write_all(script_content.as_bytes())
            .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let term_flag = Arc::new(AtomicBool::new(false));
        let should_cancel = || term_flag.load(Ordering::Relaxed);

        let (processed, errored) =
            parallelize_chunk(&files, Some(&script_path), should_cancel).unwrap();

        assert_eq!(processed, 3);
        assert_eq!(errored, 0);
    }
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
        return Err(anyhow!("Input path does not exist"));
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
