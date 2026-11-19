use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crashline::{assemble, committed_seqs, durable, durable_dir, COMMIT, PAGE};

fn main() {
    let mut args = env::args().skip(1);
    let code = match args.next().as_deref() {
        Some("child") => child(args.next()),
        Some("parent") => parent(args.next()),
        _ => {
            eprintln!("usage: crashline parent|child <wal>");
            2
        }
    };
    std::process::exit(code);
}

fn child(wal: Option<String>) -> i32 {
    let Some(wal) = wal else {
        return 2;
    };
    let path = PathBuf::from(&wal);
    let created = !path.exists();
    let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("open: {err}");
            return 1;
        }
    };
    if created {
        if let Err(err) = durable_dir(&path) {
            eprintln!("dir fsync: {err}");
            return 1;
        }
    }
    let page = assemble(COMMIT, 1);
    if let Err(err) = file.write_all(&page) {
        eprintln!("write: {err}");
        return 1;
    }
    let data_rc = match durable(&file) {
        Ok(()) => 0,
        Err(_) => 1,
    };
    let dir_rc = if created {
        match durable_dir(&path) {
            Ok(()) => 0,
            Err(_) => 1,
        }
    } else {
        0
    };
    println!("{data_rc} {dir_rc}");
    let _ = io::stdout().flush();
    let mut hold = [0u8; 1];
    let _ = io::stdin().read(&mut hold);
    let _ = PAGE;
    0
}

fn parent(wal: Option<String>) -> i32 {
    let Some(wal) = wal else {
        return 2;
    };
    let exe = env::current_exe().expect("current exe");
    let mut child = match Command::new(exe)
        .args(["child", &wal])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            eprintln!("spawn: {err}");
            return 1;
        }
    };
    let mut line = String::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = BufReader::new(stdout).read_line(&mut line);
    }
    let mut codes = line.split_whitespace();
    let data_rc = codes.next();
    let dir_rc = codes.next();
    if data_rc != Some("0") || dir_rc != Some("0") {
        let _ = child.kill();
        eprintln!("infra:fsync");
        return 3;
    }
    let _ = child.kill();
    let _ = child.wait();
    let bytes = std::fs::read(&wal).unwrap_or_default();
    println!("{:?}", committed_seqs(&bytes));
    0
}
