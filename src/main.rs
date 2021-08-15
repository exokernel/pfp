use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};

fn main() {

    let v: Vec<String> = (0..100).map(|x| x.to_string()).collect();

    let mut child = Command::new("bash").arg("-c")
        .arg("parallel -n1 -a - 'echo -n \"$$ {#} {%} {} -- \";\
              sleeptime=$(echo -n $(($RANDOM % 10)));\
              echo sleep $sleeptime;\
              sleep $sleeptime'")
        //.arg("echo $$ {#} {%} {}")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    //let string_list = vec!["Foo".to_string(),"Bar".to_string()];
    let joined = v.join("\n");
    //for s in v {
    //    println!("{}", s)
    //}
    std::thread::spawn(move || {
        stdin.write_all(joined.as_bytes()).expect("Failed to write to stdin");
    });
    
    let output = child.wait_with_output().expect("Failed to read stdout");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}