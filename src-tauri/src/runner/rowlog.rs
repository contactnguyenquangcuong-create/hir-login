//! The result file that sits beside a run's Excel/CSV list: one line per row the run took,
//! saying what came of it, so the operator can check a long run without reading the log.
//!
//! Like the `.used.txt` beside it, it is a separate file — the operator's own sheet is never
//! modified. It is a CSV with a UTF-8 byte-order mark, which is what makes Excel show Vietnamese
//! text correctly when the file is double-clicked.

use std::io::Write;

pub const HEADER: [&str; 6] = ["Thời gian", "Profile", "Giá trị", "Bình luận", "Kết quả", "Chi tiết"];

/// Where the results for `list` (the path of the Excel/CSV the rows came from) are kept.
pub fn results_path(list: &str) -> String {
    format!("{list}.results.csv")
}

/// One CSV line: every field quoted, quotes doubled. Newlines inside a field are flattened,
/// so one row of the file is always one result.
pub fn csv_line(fields: &[&str]) -> String {
    let cells: Vec<String> = fields
        .iter()
        .map(|f| format!("\"{}\"", f.replace(['\r', '\n'], " ").replace('"', "\"\"")))
        .collect();
    cells.join(",")
}

/// Appends one result. The file is created, with its header, on the first one.
pub fn append(list: &str, fields: &[&str; 6]) -> std::io::Result<()> {
    let path = results_path(list);
    let fresh = !std::path::Path::new(&path).exists();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    if fresh {
        f.write_all(b"\xEF\xBB\xBF")?;
        writeln!(f, "{}", csv_line(&HEADER))?;
    }
    writeln!(f, "{}", csv_line(fields))
}

pub use crate::localtime::local_stamp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_lines_survive_quotes_commas_and_newlines() {
        assert_eq!(csv_line(&["a", "b,c", "say \"hi\"", "two\nlines"]), "\"a\",\"b,c\",\"say \"\"hi\"\"\",\"two lines\"");
    }

    #[test]
    fn the_file_starts_with_a_bom_and_a_header_then_grows_by_one_line_per_result() {
        let dir = std::env::temp_dir().join(format!("hir-rowlog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("danh sach.xlsx").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(results_path(&list));
        append(&list, &["2026-10-07 20:00:00", "p1", "https://x/1", "hay quá", "OK", ""]).unwrap();
        append(&list, &["2026-10-07 20:01:00", "p2", "https://x/2", "", "LỖI", "step 3 — no post"]).unwrap();
        let bytes = std::fs::read(results_path(&list)).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"), "BOM so Excel reads UTF-8");
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].contains("Kết quả"));
        assert!(lines[1].contains("hay quá") && lines[1].contains("\"OK\""));
        assert!(lines[2].contains("LỖI") && lines[2].contains("step 3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
