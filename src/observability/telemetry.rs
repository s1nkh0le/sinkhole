use super::eval_record::EvaluationRecord;
use std::io::{self, Write};

pub fn emit_record(rec: &EvaluationRecord) {
    if let Ok(line) = serde_json::to_string(rec) {
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "{}", line);
    }
}
