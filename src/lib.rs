use anyhow::{Context, Result};
use log::{debug, error};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::{fs, io};
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
/// let command = "echo";
/// let term = Arc::new(AtomicBool::new(false));
/// let (num_processed, num_errored) = parallelize_chunk(&chunk, command, &term).expect("Failed to process chunk");
/// println!("Processed: {}, Errored: {}", num_processed, num_errored);
/// ```
pub fn parallelize_chunk(
    chunk: &[PathBuf],
    command: &str,
    term: &Arc<AtomicBool>,
) -> Result<(usize, usize)> {
    let processed = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);

    chunk.par_iter().try_for_each(|file| -> Result<()> {
        if should_term(term) {
            log::info!("Cancelling task for file: {}", file.to_string_lossy());
            return Ok(());
        }

        let output = Command::new(command)
            .arg(file)
            .output()
            .with_context(|| format!("Failed to execute command for file: {}", file.display()))?;

        if output.status.success() {
            debug!("Processed file: {}", file.to_string_lossy());
            debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            processed.fetch_add(1, Ordering::Relaxed);
        } else {
            error!("Command failed for file: {}", file.to_string_lossy());
            error!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            errors.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    })?;

    Ok((
        processed.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    ))
}

/// Fill a vector with all the file paths under the given dir (recursive)
/// that have the given extensions
///
/// # Arguments
///
/// * `dir` - A reference to the `Path` representing the directory to search.
/// * `extensions` - A vector of string slices representing file extensions to filter by.
/// * `files` - A mutable reference to a `Vec<String>` where matching file paths will be added.
///
/// # Returns
///
/// Returns an `io::Result<()>` indicating success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if it encounters any issues reading the directory or its entries.
///
/// # Note
///
/// If the `extensions` vector is empty, all files in the directory will be included.
pub fn get_files(dir: &Path, extensions: &Vec<&str>, files: &mut Vec<String>) -> io::Result<()> {
    if dir.is_dir() {
        for e in fs::read_dir(dir)? {
            let entry = e?;
            let path = entry.path();
            if path.is_dir() {
                debug!("D {:?}", path);
                get_files(&path, extensions, files)?;
            } else if extensions.is_empty()
                || extensions.contains(&path.extension().unwrap().to_str().unwrap())
            {
                debug!("f {:?}", path);
                files.push(path.display().to_string());
            }
        }
    }
    Ok(())
}

/// Recursively traverses a directory and applies a handler function to each file found.
///
/// This function takes a directory path, and a mutable
/// closure that handles each file found. It recursively traverses the directory and its
/// subdirectories, applying the handler function to each file.
///
/// # Arguments
///
/// * `dir` - A reference to the path of the directory to traverse.
/// * `file_handler` - A mutable closure that takes a reference to a file path. The closure is responsible for handling
///   each file found.
///
/// # Returns
///
/// This function returns an `io::Result<()>` indicating success or failure.
///
/// # Errors
///
/// This function will return an error if it encounters any issues reading the directory or its
/// entries.
pub fn get_files2<F>(dir: &Path, file_handler: &mut F) -> io::Result<()>
where
    F: FnMut(&Path),
{
    if dir.is_dir() {
        for e in fs::read_dir(dir)? {
            let entry = e?;
            let path = entry.path();
            if path.is_dir() {
                debug!("D {:?}", path);
                get_files2(&path, file_handler)?;
            } else {
                debug!("f {:?}", path);
                file_handler(&path);
            }
        }
    }

    Ok(())
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
pub fn get_files3(input_path: &Path, extensions: &Option<Vec<&OsStr>>) -> Result<Vec<PathBuf>> {
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

        let files = get_files3(temp_dir.path(), &extensions).unwrap();

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

        let files = get_files3(temp_dir.path(), &extensions).unwrap();

        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files3_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let extensions = None;

        let files = get_files3(temp_dir.path(), &extensions).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn test_get_files3_non_existent_directory() {
        let non_existent_path = Path::new("/this/path/does/not/exist");
        let extensions = None;

        let result = get_files3(non_existent_path, &extensions);

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
