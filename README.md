# Chant-shell (CSC 308 Mini Project)

Chant-shell is a small shell written in Rust using only the standard library. It reads commands from standard input, one per line, runs them, and prints only what the programs print. It never prints a prompt, and every error from the shell itself goes to standard error.

## Build and run

```
cargo build --release
./target/release/shell_os < commands.txt
```

Run the unit tests with `cargo test`.

## Usage examples

Start the shell and type commands. There is no prompt, so the shell just waits for input. Type `exit` or press Ctrl+D to quit.

```
./target/release/shell_os
```

Any program installed on the system can be run, including `mkdir` and `ls`. They are ordinary programs, not built-ins:

| You type | Output |
|---|---|
| `mkdir notes` | (creates the folder `notes`) |
| `ls` | `notes` |
| `cd notes` | (moves into `notes`) |
| `echo hello > a.txt` | (writes `hello` into `a.txt`) |
| `ls` | `a.txt` |
| `cat a.txt` | `hello` |
| `cd ..` | (moves back up) |
| `ls notes \| wc -l` | `1` |
| `mkdir notes` | an error from `mkdir` that the folder already exists (on standard error; the shell keeps going) |

The same commands can be saved in a file and run all at once with `./target/release/shell_os < commands.txt`.

## What it supports

- **Programs with arguments**, for example `echo hello world` or `cat file.txt`. Words are split on spaces and tabs. The shell waits for each program to finish before it reads the next line.
- **`cd DIR`** (built in). It changes the shell's own working directory with `std::env::set_current_dir`. A missing directory, or `cd` with no directory, prints an error on standard error and the shell carries on.
- **`exit`** (built in). It stops the shell. The shell also stops when its input ends.
- **Output redirection** at the end of a command: `echo hi > f.txt` creates the file, or empties it if it already exists, and the program writes into it instead of the screen.
- **One pipe**: `cmd1 args | cmd2 args`, which may end with a redirection, for example `cat a b | sort > out.txt`.
- **Blank lines** and lines with only spaces are skipped. An unknown command prints an error on standard error and the shell keeps going.

## How it works

`main` reads standard input line by line. `run_line` splits a line into words and cuts it at `|`. With no pipe, `run_single` removes a trailing `> file` (`split_redirect`), then handles `exit` and `cd` itself or runs the program with `std::process::Command`. `.status()` starts the program and waits for it.

For a redirection, the file is opened with `File::create` and passed to the program as its standard output with `Stdio::from(file)`. The program writes to standard output as usual, and the operating system puts the data in the file.

For a pipe, `run_pipe` starts the left program with `spawn()` and `Stdio::piped()`, so its output goes into an operating-system pipe and the shell does not wait yet. The right program gets the read end of that pipe as its standard input and is run with `.status()`. Afterwards the shell also waits for the left program, so no zombie process is left behind. The shell's own copy of the pipe is dropped as soon as the right program ends, so a left program such as `yes` in `yes | head -2` gets an error when it writes and stops instead of blocking forever.

Every program the shell starts gets an empty standard input. The shell's standard input holds the rest of the commands, and a program like `cat` with no file must not read them.

## Why `cd` and `exit` are built in

Every process has its own current directory. A program started by the shell is a child process with a copy of the shell's directory. If `cd` were an ordinary program, it would change only its own directory and then exit, and the shell's directory would not change. So the shell must call `set_current_dir` on itself. In the same way, a separate `exit` program could only end itself, not the shell.

## Testing

- **Public tests.** The course self-check script builds the project and runs the six public shell tests. Each test runs in a fresh, empty folder and only standard output is compared:

   ```
   python3 selfcheck.py --project . --track shell --tests tests
   ```

   Result: 6/6 public tests pass.

- **Unit tests:** `cargo test` checks `split_redirect` with normal and malformed redirections (3/3 pass).
- **Edge cases:** empty input, blank lines, a bad `cd`, unknown commands (also inside a pipe) and `yes | head -3` all behave like bash, with errors only on standard error.

## Known limitations

- No quotes or escape characters: `echo "a b"` passes `"a` and `b"` as two words.
- `>` and `|` must be separate words with spaces around them (`echo hi >f.txt` is not a redirection).
- Only one pipe per line, and only the last command may redirect. No `>>` append and no input redirection with `<`.
- `cd` and `exit` are only built in when they are not part of a pipe.
