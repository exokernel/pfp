# Parallel File Processor (PFP)

## Overview

Parallel File Processor (PFP) is a Rust-based utility that recursively reads a directory and executes a specified command on each file in parallel. It's designed for efficient processing of large numbers of files by leveraging parallel execution.

This is useful when you have a script that does some processing on a single file but you want to process a lot of files under some directory efficiently. The original use case was for running a script to compress, encrypt, and upload files to a cloud storage service, on a very large number of video files.

## Features

- Recursive directory traversal
- Parallel file processing using the Rayon crate
- Chunked file processing for better memory management
- Optional file extension filtering
- Customizable command execution for each file
- Daemon mode for continuous processing
- Detailed logging and error reporting

## Usage

```
pfp [OPTIONS] <input_path>
```

### Options

- `-d, --debug`: Activate debug mode
- `--daemon`: Process files in input path continuously
- `-e, --extensions <EXTENSIONS>`: List of file extensions to process (comma-separated)
- `-c, --chunk-size <CHUNK_SIZE>`: Number of files to process in parallel at once (default: 50)
- `-j, --job-slots <JOB_SLOTS>`: Number of parallel job slots to use
- `-t, --sleep-time <SLEEP_TIME>`: Seconds to sleep before reprocessing in daemon mode (default: 5)
- `-s, --script <SCRIPT>`: Script to run on each file

### Examples

Process all files in a directory:
```
pfp /path/to/directory
```

Process only MP4 and FLV files:
```
pfp -e "mp4,flv" /path/to/directory
```

Use a custom command (note the command should take a single file path as its argument):
```
pfp -s /path/to/script.sh /path/to/videos
```

## Installation

For now you can build the binary yourself. I'll try to get an actual release out soon.

* Clone the repo
* Install rust with rustup, super easy! (https://rustup.rs/)
* Go into the project and do: `cargo build --release`
* Find the optimized `pfp` binary at `./target/release/pfp` and go nuts!

## Contributing

Contributions are welcome! This was one of my first real Rust projects, so suggestions for improvement are appreciated. Please feel free to submit issues or pull requests.
