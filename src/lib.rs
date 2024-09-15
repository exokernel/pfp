use log::debug;
use std::error::Error;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// Executes a command in parallel using GNU Parallel with the given input.
///
/// This function runs parallel instances of the specified command, with one item of input per instance.
///
/// # Arguments
///
/// * `command` - A string slice containing the command to be executed in parallel.
/// * `job_slots` - A string slice specifying the number of job slots to use. Use "100%" for maximum parallelism.
/// * `input` - A vector of strings, where each string is an input item for a parallel instance of the command.
///
/// # Returns
///
/// Returns a `Result<(), Box<dyn Error>>` indicating success or failure of the operation.
///
/// # Errors
///
/// This function may return an error if there are issues spawning the parallel process,
/// writing to its stdin, or waiting for it to complete.
///
/// # Deprecated
///
/// This function is deprecated and will be removed in a future version.
/// Consider using alternative parallelization methods.
#[deprecated(
    since = "0.1.1",
    note = "This function is deprecated and will be removed in a future version. Consider using alternative parallelization methods."
)]
pub fn parallelize(
    command: &str,
    job_slots: &str,
    input: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let mut parallel_args: Vec<String> = vec![];
    if job_slots != "100%" {
        parallel_args.push(format!("-j{}", job_slots));
    }
    parallel_args.push(command.to_owned());

    debug!("parallelizing with {} job slots", job_slots);

    let mut child = Command::new("parallel")
        .args(parallel_args)
        .stdin(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().ok_or("Failed to open stdin")?;
    std::thread::spawn(move || {
        stdin
            .write_all(input.join("\n").as_bytes())
            .expect("Failed to write to stdin");
    });

    child.wait()?;

    Ok(())
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
/// * `extensions` - An optional `Vec<&str>` containing file extensions to filter by.
///                  If `None`, all files are included.
///
/// # Returns
///
/// Returns a `Result` containing a `Vec<PathBuf>` of matching file paths on success,
/// or a boxed dynamic `Error` on failure.
///
/// # Errors
///
/// This function may return an error if there are issues with file system operations
/// or directory traversal.
pub fn get_files3(
    input_path: &Path,
    extensions: &Option<Vec<&str>>,
) -> Result<Vec<PathBuf>, Box<dyn Error + Send + Sync>> {
    let mut files = Vec::new();

    let should_include = |file_path: &Path| -> bool {
        if let Some(exts) = extensions {
            file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| exts.contains(&ext))
                .unwrap_or(false)
        } else {
            true
        }
    };

    // TODO: parallelize this with rayon!
    for entry in WalkDir::new(input_path)
        .follow_links(true)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() && should_include(entry.path()) {
            files.push(entry.path().to_path_buf());
        }
    }

    Ok(files)
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
