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

/// Execute command in a subprocess using Gnu Parallel with given input
/// Runs parallel instances of command with one item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
//fn parallelize(command: &str, job_slots: &str, input: Vec<String>) -> Output {
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
/// DONE (in get_files2): Would be better to pass a closure for adding files to the list
/// Instead of checking if extensions is empty every iteration let the caller decide what determines
/// if a file gets appended to the list. It could pass one closure that unconditionally adds files if
/// the list of extensions is empty, otherwise a closure that checks the file against the extensions list
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

pub fn should_term(term: &Arc<AtomicBool>) -> bool {
    if term.load(Ordering::Relaxed) {
        log::info!("PFP: CAUGHT SIGNAL! K Thx Bye!");
        return true;
    }
    false
}
