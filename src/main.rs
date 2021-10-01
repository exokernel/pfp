use std::io::{Write};
use std::io;
use std::process::{Command, Stdio, Output};
//use std::error::Error;
use structopt::StructOpt;
use log::{debug};
use std::fs;
use std::path::Path;

#[derive(Debug, StructOpt)]
#[structopt(name = "parallel-test", about = "Do some things in parallel")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Activate test mode
    #[structopt(short, long)]
    test: bool,

    /// Number of things to try to do in parallel at one time.
    /// This is the number inputs that will be fed to a single invocation of
    /// Gnu Parallel. The actual number of parallel jobs per chunk is limited
    /// by job_slots.
    #[structopt(short, long, default_value = "50")]
    chunk_size: usize,

    /// Number of parallel job slots to use. Default to 1 slot per CPU core.
    #[structopt(short, long)]
    job_slots: Option<usize>,

    /// Shell command or script to run in parallel
    #[structopt(short, long)]
    script: Option<String>,

    /// Directory to read files from
    input_path: String,
}

/// Execute command in a subprocess using Gnu Parallel with given input
/// Runs parallel instances of command with on item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
fn parallelize(command: &str, input: Vec<String>) -> Output {

    let mut child = Command::new("parallel")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(input.join("\n").as_bytes()).expect("Failed to write to stdin");
    });

    return child.wait_with_output().expect("Failed to read stdout");
}

fn getFiles(dir: &Path) -> io::Result<()> {
    //let mut files: Vec<&Path> = vec![];
    if dir.is_dir() {
        // print info about each dir ent
        for e in fs::read_dir(dir)? {
            let entry = e?;
            let path = entry.path();
            if path.is_dir() {
                debug!("D {:?}", path);
            } else {
                debug!("f {:?}", path);
                //files.push(&path)
            }
        }
    }
    Ok(())
}

fn main() {

    let opt = Opt::from_args();
    if opt.debug {
        std::env::set_var("RUST_LOG", "debug");
    }

    // By default we'll use -j 100% to run one job per CPU core
    let mut job_slots = String::from("100%");
    if ! opt.job_slots.is_none() {
        job_slots = opt.job_slots.unwrap().to_string();
    }

    env_logger::init();

    debug!("{:?}", opt);
    debug!("job_slots = {}", job_slots);

    loop {

        // Get all the files in our input path
        let files = getFiles(Path::new(&opt.input_path));

        // 1. get a whole set of input (e.g. names of all files in some directory)
        //    if there's no input sleep for a while and try again
        //let v: Vec<String> = (0..101).map(|x| x.to_string()).collect();

        // 2. process chunks of input in parallel
        //let chunksize = 50;
        //let numchunks = v.len() / chunksize; // 2
        //let leftover  = v.len() % chunksize; // 1
        //println!("number of chunks {}", numchunks);
        //println!("leftover {}", leftover);

        // ??? how to break input up into chunks

        // Each chunk is a reference to slice of our Vec<String>:  &[String] &["foo", "bar",...,"baz"]
        // chunk 1:  [0..50] 0-49 50-things
        // chunk 2:  [50..100] 50-99 50-things
        // chunk 3:  [100..101] 100 1-thing

        /*
        for i in 0..numchunks {
            println!("chunk {}", i + 1);
            let index = chunksize*i;
            let chunk = (&v[index..(index+chunksize)]).to_vec();
            let output = parallelize("echo {#}-{%}-{}", chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }
        // last chunk
        if leftover != 0 {
            println!("chunk {}", numchunks + 1);
            let index = chunksize*numchunks;
            let chunk = (&v[index..(index+leftover)]).to_vec();
            let output = parallelize("echo {#}-{%}-{}", chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }
        */

        break;
        // 3. Do any necessary postprocessing
    }
}
