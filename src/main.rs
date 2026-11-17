//! A mini shell for CSC 308.
//! Reads commands from standard input, one per line, and runs them.
//! Supports: programs with arguments, built-in cd and exit,
//! "> file" output redirection and one "|" pipe.
//! Only program output goes to stdout; all shell errors go to stderr.

use std::fs::File;
use std::io::{self, BufRead};
use std::process::{Child, Command, Stdio};

fn main() {
    let stdin = io::stdin();

    // Read the input one line at a time. The loop ends when the input ends.
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(text) => text,
            Err(_) => break, // could not read input: stop the shell
        };

        // run_line returns false when the shell should stop (exit).
        if !run_line(&line) {
            break;
        }
    }
}

/// Run one input line. Returns false if the shell must stop.
fn run_line(line: &str) -> bool {
    // Split the line into words on any whitespace (spaces or tabs).
    let words: Vec<&str> = line.split_whitespace().collect();

    // A blank line (or only spaces) has no words: skip it.
    if words.is_empty() {
        return true;
    }

    // Cut the words at every "|". No pipe gives 1 part, one pipe gives 2.
    let parts: Vec<&[&str]> = words.split(|w| *w == "|").collect();

    match parts.len() {
        1 => return run_single(&words),
        2 => run_pipe(parts[0], parts[1]),
        _ => eprintln!("shell: only one pipe is supported"),
    }
    true
}

/// Run a line without a pipe: a built-in or one program.
/// Returns false if the command was exit.
fn run_single(words: &[&str]) -> bool {
    // Separate an output redirection ("> file") from the command words.
    let (cmd_words, out_file) = match split_redirect(words) {
        Some(p) => p,
        None => return true, // the error was already printed
    };

    // First word is the program, the rest are its arguments.
    let program = cmd_words[0];
    let args = &cmd_words[1..];

    // Built-in commands are handled by the shell itself.
    match program {
        "exit" => return false,
        "cd" => change_dir(args),
        _ => run_command(program, args, out_file),
    }
    true
}

/// Look for "> file" at the end of a command.
/// Returns the command words and the file name (if there is one),
/// or None after printing an error if the redirection is malformed.
fn split_redirect<'a>(words: &[&'a str]) -> Option<(Vec<&'a str>, Option<&'a str>)> {
    match words.iter().position(|w| *w == ">") {
        // No ">": the whole line is the command.
        None => Some((words.to_vec(), None)),

        Some(i) => {
            // There must be a command before ">".
            if i == 0 {
                eprintln!("shell: missing command before >");
                return None;
            }
            // ">" must be followed by exactly one word: the file name.
            if i + 2 != words.len() {
                eprintln!("shell: expected one file name after >");
                return None;
            }
            Some((words[..i].to_vec(), Some(words[i + 1])))
        }
    }
}

/// Built-in cd: change the shell's OWN working directory.
/// It cannot be a separate program, because a child process can only
/// change its own directory, not the shell's.
fn change_dir(args: &[&str]) {
    let dir = match args.first() {
        Some(d) => *d,
        None => {
            eprintln!("shell: cd: missing directory");
            return;
        }
    };

    if let Err(e) = std::env::set_current_dir(dir) {
        eprintln!("shell: cd: {}: {}", dir, e);
    }
}

/// Open a file for an output redirection: create it, or empty it if it exists.
/// Returns None after printing an error if it cannot be opened.
fn open_output(path: &str) -> Option<File> {
    match File::create(path) {
        Ok(file) => Some(file),
        Err(e) => {
            eprintln!("shell: {}: {}", path, e);
            None
        }
    }
}

/// Build a Command for a program and its arguments.
/// Its stdin is empty (null): the shell's own stdin holds the script,
/// and a child program must not read our remaining command lines.
fn make_command(words: &[&str]) -> Command {
    let mut cmd = Command::new(words[0]);
    cmd.args(&words[1..]).stdin(Stdio::null());
    cmd
}

/// Start a program and wait for it to finish.
/// If out_file is given, the program's output goes to that file
/// (created, or emptied if it already exists) instead of our stdout.
fn run_command(program: &str, args: &[&str], out_file: Option<&str>) {
    let mut words = vec![program];
    words.extend_from_slice(args);
    let mut cmd = make_command(&words);

    if let Some(path) = out_file {
        match open_output(path) {
            Some(file) => {
                cmd.stdout(Stdio::from(file));
            }
            None => return,
        }
    }

    // .status() starts the program and waits for it to finish.
    if let Err(e) = cmd.status() {
        eprintln!("shell: {}: {}", program, e);
    }
}

/// Run "left | right", where right may end with "> file".
/// The left program's stdout is connected to the right program's stdin.
fn run_pipe(left: &[&str], right: &[&str]) {
    // Both sides need a command, and only the right side may redirect.
    if left.is_empty() || right.is_empty() {
        eprintln!("shell: missing command next to |");
        return;
    }
    if left.contains(&">") {
        eprintln!("shell: > is only allowed at the end of the line");
        return;
    }
    let (right_words, out_file) = match split_redirect(right) {
        Some(p) => p,
        None => return,
    };

    // 1. Start the left program with its stdout going into a new pipe.
    //    spawn() does NOT wait, so both programs can run at the same time.
    let mut first: Option<Child> = match make_command(left).stdout(Stdio::piped()).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("shell: {}: {}", left[0], e);
            None // the right side still runs, with empty input
        }
    };

    // 2. The right program reads from the read end of that pipe
    //    (or from nothing, if the left program could not start).
    let right_input = match first.as_mut().and_then(|c| c.stdout.take()) {
        Some(pipe) => Stdio::from(pipe),
        None => Stdio::null(),
    };

    // 3. Run the right program and wait for it. The block makes sure the
    //    Command (and our copy of the pipe inside it) is dropped right after,
    //    so the left program is not kept waiting on a pipe nobody reads.
    {
        let mut second = make_command(&right_words);
        second.stdin(right_input);

        let mut can_run = true;
        if let Some(path) = out_file {
            match open_output(path) {
                Some(file) => {
                    second.stdout(Stdio::from(file));
                }
                None => can_run = false,
            }
        }

        if can_run {
            if let Err(e) = second.status() {
                eprintln!("shell: {}: {}", right_words[0], e);
            }
        }
    }

    // 4. Wait for the left program as well, so it does not stay a zombie.
    if let Some(mut child) = first {
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_redirect() {
        let (cmd, file) = split_redirect(&["echo", "hi"]).unwrap();
        assert_eq!(cmd, vec!["echo", "hi"]);
        assert_eq!(file, None);
    }

    #[test]
    fn redirect_at_end() {
        let (cmd, file) = split_redirect(&["echo", "hi", ">", "f.txt"]).unwrap();
        assert_eq!(cmd, vec!["echo", "hi"]);
        assert_eq!(file, Some("f.txt"));
    }

    #[test]
    fn bad_redirects() {
        assert!(split_redirect(&["echo", "hi", ">"]).is_none()); // no file
        assert!(split_redirect(&[">", "f.txt"]).is_none()); // no command
        assert!(split_redirect(&["echo", ">", "a", "b"]).is_none()); // two files
    }
}
