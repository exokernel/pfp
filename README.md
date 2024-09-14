Parallel File Processor

Reads a directory recursively and executes a command on each file. Files are processed in chunks and the command is executed
in parallel for each file in the chunk. Uses the rayon crate for parallelism.

This was one of my first real Rust projects so it might not be the greatest Rust ever. Suggestions for improvement welcome!
