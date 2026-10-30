// G0 floor probe: measures build and process-startup cost on the board.
// Deliberately NOT a solution: reads nothing from /data and writes an empty file.
use std::fs::File;

fn main() {
    // Creating the file and dropping the handle closes it; exit code is 0.
    File::create("/out/summary.txt").expect("cannot create /out/summary.txt");
}
