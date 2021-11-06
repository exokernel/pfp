use std::io::{Write};
use std::io;
use std::process::{Command, Stdio};
use std::fs;
use std::path::{Path};
use chrono::prelude::*;
use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use log::{debug};
use std::error::Error;

enum Fd {
    StdOut,
    StdErr,
}

/// Print to stdout w timestamp
pub fn print(message: &str) {
   tp(message, Fd::StdOut);
}

/// Print to stderr w timestamp
pub fn eprint(message: &str) {
    tp(message, Fd::StdErr);
}

/// Print with timestamp
fn tp(message: &str, fd: Fd) {
    let local: DateTime<Local> = Local::now();
    match fd {
        Fd::StdOut => {
            println!("{}: {}", local.to_string(), message);
        },
        Fd::StdErr => {
            eprintln!("{}: {}", local.to_string(), message);
        }
    }
}

/// Execute command in a subprocess using Gnu Parallel with given input
/// Runs parallel instances of command with one item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
//fn parallelize(command: &str, job_slots: &str, input: Vec<String>) -> Output {
pub fn parallelize(command: &str, job_slots: &str, input: Vec<String>)
   -> Result<(), Box<dyn Error>> {

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
        stdin.write_all(input.join("\n").as_bytes()).expect("Failed to write to stdin");
    });

    child.wait()?;

    Ok(())
}

/// Fill a vector with all the file paths under the given dir (recursive)
/// that have the given extensions
/// TODO: Would be better to pass a closure for adding files to the list
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
            } else if extensions.is_empty() {
                debug!("f {:?}", path);
                files.push(path.display().to_string());
            } else if
              path.extension().is_some() &&
              extensions.contains(&path.extension().unwrap()
                                       .to_str().unwrap()) {
                debug!("f {:?}", path);
                files.push(path.display().to_string());
            }
        }
    }
    Ok(())
}

pub fn should_term(term: &Arc<AtomicBool>) -> bool {
    if term.load(Ordering::Relaxed) {
        print(format!("PFP: CAUGHT SIGNAL! K Thx Bye!").as_str());
        return true;
    }
    false
}

pub fn get_chunk(start: usize, num_items: usize, files: &Vec<String>) -> Vec<String>{
    let end = start+num_items;
    files[start..end].to_vec()
}